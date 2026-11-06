//! Sync server for SillyTavern RS: a small revisioned key/value store over HTTP.
//!
//! Every item (character card, chat, settings, encrypted API keys, persona avatar)
//! has a revision. Writes must name the revision they are based on; a mismatch
//! returns 409 so the app can ask the user which copy to keep. A global sequence
//! number lets clients fetch only what changed since their last sync.
//! The server never sees API keys in plain text: clients encrypt them first.

mod ingest;

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub const KINDS: &[&str] = &["character", "chat", "settings", "persona_avatar", "secrets"];
pub const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;

pub struct Server {
    db: Mutex<Connection>,
    token: String,
    server_id: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ItemMeta {
    pub kind: String,
    pub id: String,
    pub rev: i64,
    pub seq: i64,
    pub hash: String,
    pub deleted: bool,
    pub size: i64,
    pub updated_at: i64,
}

fn random_hex(bytes: usize) -> Result<String, String> {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).map_err(|e| format!("random source failed: {e}"))?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

impl Server {
    /// Opens (or creates) `sync.db` and the access token in `data_dir`.
    /// Returns the server and whether a new token was generated.
    pub fn open(data_dir: &Path, token_override: Option<String>) -> Result<(Self, bool), String> {
        std::fs::create_dir_all(data_dir).map_err(|e| format!("cannot create {}: {e}", data_dir.display()))?;
        let db = Connection::open(data_dir.join("sync.db")).map_err(|e| e.to_string())?;
        db.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS items (
                kind TEXT NOT NULL, id TEXT NOT NULL, rev INTEGER NOT NULL, seq INTEGER NOT NULL,
                hash TEXT NOT NULL, deleted INTEGER NOT NULL DEFAULT 0, data BLOB,
                size INTEGER NOT NULL DEFAULT 0, updated_at INTEGER NOT NULL,
                PRIMARY KEY (kind, id));
             CREATE INDEX IF NOT EXISTS items_seq ON items(seq);
             CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )
        .map_err(|e| e.to_string())?;

        let server_id: Option<String> = db
            .query_row("SELECT value FROM meta WHERE key = 'server_id'", [], |r| r.get(0))
            .optional()
            .map_err(|e| e.to_string())?;
        let server_id = match server_id {
            Some(id) => id,
            None => {
                let id = random_hex(16)?;
                db.execute("INSERT INTO meta (key, value) VALUES ('server_id', ?1)", params![id])
                    .map_err(|e| e.to_string())?;
                id
            }
        };

        let token_file = data_dir.join("token");
        let mut generated = false;
        let token = match token_override.filter(|t| !t.trim().is_empty()) {
            Some(t) => t.trim().to_string(),
            None => match std::fs::read_to_string(&token_file) {
                Ok(t) if !t.trim().is_empty() => t.trim().to_string(),
                _ => {
                    let t = random_hex(24)?;
                    std::fs::write(&token_file, &t).map_err(|e| e.to_string())?;
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let _ = std::fs::set_permissions(&token_file, std::fs::Permissions::from_mode(0o600));
                    }
                    generated = true;
                    t
                }
            },
        };
        Ok((Self { db: Mutex::new(db), token, server_id }, generated))
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|e| e.into_inner())
    }
}

struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<rusqlite::Error> for ApiError {
    fn from(e: rusqlite::Error) -> Self {
        ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    }
}

type ApiResult<T> = Result<T, ApiError>;

fn validate(kind: &str, id: &str) -> ApiResult<()> {
    if !KINDS.contains(&kind) {
        return Err(ApiError(StatusCode::BAD_REQUEST, format!("unknown kind: {kind}")));
    }
    if id.is_empty() || id.len() > 512 || id.contains('\0') {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid id".into()));
    }
    Ok(())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn require_token(State(server): State<Arc<Server>>, request: Request, next: Next) -> Response {
    let presented = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    match presented {
        Some(token) if constant_time_eq(token.trim().as_bytes(), server.token.as_bytes()) => next.run(request).await,
        _ => ApiError(StatusCode::UNAUTHORIZED, "invalid or missing access token".into()).into_response(),
    }
}

fn read_meta(db: &Connection, kind: &str, id: &str) -> rusqlite::Result<Option<ItemMeta>> {
    db.query_row(
        "SELECT kind, id, rev, seq, hash, deleted, size, updated_at FROM items WHERE kind = ?1 AND id = ?2",
        params![kind, id],
        row_meta,
    )
    .optional()
}

fn row_meta(r: &rusqlite::Row<'_>) -> rusqlite::Result<ItemMeta> {
    Ok(ItemMeta {
        kind: r.get(0)?,
        id: r.get(1)?,
        rev: r.get(2)?,
        seq: r.get(3)?,
        hash: r.get(4)?,
        deleted: r.get::<_, i64>(5)? != 0,
        size: r.get(6)?,
        updated_at: r.get(7)?,
    })
}

async fn health() -> Json<Value> {
    Json(json!({ "ok": true, "version": env!("CARGO_PKG_VERSION") }))
}

async fn info(State(server): State<Arc<Server>>) -> ApiResult<Json<Value>> {
    let latest: i64 = server.db().query_row("SELECT COALESCE(MAX(seq), 0) FROM items", [], |r| r.get(0))?;
    Ok(Json(json!({ "server_id": server.server_id, "latest_seq": latest, "version": env!("CARGO_PKG_VERSION") })))
}

#[derive(Deserialize)]
struct ChangesQuery {
    since: Option<i64>,
}

async fn changes(State(server): State<Arc<Server>>, Query(q): Query<ChangesQuery>) -> ApiResult<Json<Value>> {
    let db = server.db();
    let latest: i64 = db.query_row("SELECT COALESCE(MAX(seq), 0) FROM items", [], |r| r.get(0))?;
    let mut stmt = db.prepare(
        "SELECT kind, id, rev, seq, hash, deleted, size, updated_at FROM items WHERE seq > ?1 ORDER BY seq",
    )?;
    let items = stmt.query_map(params![q.since.unwrap_or(0)], row_meta)?.collect::<Result<Vec<_>, _>>()?;
    Ok(Json(json!({ "latest_seq": latest, "items": items })))
}

#[derive(Deserialize)]
struct ItemQuery {
    kind: String,
    id: String,
}

async fn get_item(State(server): State<Arc<Server>>, Query(q): Query<ItemQuery>) -> ApiResult<Response> {
    validate(&q.kind, &q.id)?;
    let row: Option<(i64, String, i64, Option<Vec<u8>>)> = server
        .db()
        .query_row(
            "SELECT rev, hash, deleted, data FROM items WHERE kind = ?1 AND id = ?2",
            params![q.kind, q.id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let Some((rev, hash, deleted, data)) = row else {
        return Err(ApiError(StatusCode::NOT_FOUND, "no such item".into()));
    };
    let mut headers = HeaderMap::new();
    headers.insert("x-rev", HeaderValue::from(rev));
    headers.insert("x-deleted", HeaderValue::from(deleted));
    if let Ok(v) = HeaderValue::from_str(&hash) {
        headers.insert("x-hash", v);
    }
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/octet-stream"));
    Ok((headers, data.unwrap_or_default()).into_response())
}

#[derive(Deserialize)]
struct WriteQuery {
    kind: String,
    id: String,
    base_rev: i64,
    #[serde(default)]
    hash: String,
}

async fn put_item(State(server): State<Arc<Server>>, Query(q): Query<WriteQuery>, body: Bytes) -> ApiResult<Response> {
    write(&server, &q, Some(&body))
}

async fn delete_item(State(server): State<Arc<Server>>, Query(q): Query<WriteQuery>) -> ApiResult<Response> {
    write(&server, &q, None)
}

/// Optimistic concurrency: the write only lands when `base_rev` is the current revision.
fn write(server: &Server, q: &WriteQuery, data: Option<&[u8]>) -> ApiResult<Response> {
    validate(&q.kind, &q.id)?;
    let mut db = server.db();
    let tx = db.transaction()?;
    let current = read_meta(&tx, &q.kind, &q.id)?;
    let current_rev = current.as_ref().map_or(0, |m| m.rev);
    if current_rev != q.base_rev {
        return Ok((StatusCode::CONFLICT, Json(json!({ "error": "conflict", "current": current }))).into_response());
    }
    let rev = current_rev + 1;
    let hash = if data.is_some() { q.hash.as_str() } else { "" };
    let seq = store(&tx, &q.kind, &q.id, rev, hash, data)?;
    tx.commit()?;
    Ok(Json(json!({ "rev": rev, "seq": seq })).into_response())
}

/// Writes one item revision (`data: None` marks it deleted) and returns its sequence number.
fn store(tx: &Transaction, kind: &str, id: &str, rev: i64, hash: &str, data: Option<&[u8]>) -> rusqlite::Result<i64> {
    let seq: i64 = tx.query_row("SELECT COALESCE(MAX(seq), 0) + 1 FROM items", [], |r| r.get(0))?;
    let (deleted, size) = match data {
        Some(bytes) => (0, bytes.len() as i64),
        None => (1, 0),
    };
    tx.execute(
        "INSERT OR REPLACE INTO items (kind, id, rev, seq, hash, deleted, data, size, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![kind, id, rev, seq, hash, deleted, data, size, now_ms()],
    )?;
    Ok(seq)
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// `POST /api/v1/characters`: adds a character card sent from outside the apps (a script, a
/// browser extension, curl). The body is a PNG card or a JSON card; the format is detected from
/// the content. It is stored as a normal `character` item, so every device receives it on its
/// next sync. A card never overwrites another one: a taken name gets a number (`Mira1.png`).
async fn import_character(State(server): State<Arc<Server>>, body: Bytes) -> ApiResult<Response> {
    let card = ingest::prepare(&body).map_err(|e| ApiError(StatusCode::UNPROCESSABLE_ENTITY, e))?;
    let hash = sha256_hex(&card.png);
    let stem = ingest::file_stem(&card.name);
    let mut db = server.db();
    let tx = db.transaction()?;
    // First free name: `stem.png`, `stem1.png`, ... A deleted card's name is free again.
    let mut counter = 0;
    let (id, rev) = loop {
        let id = if counter == 0 { format!("{stem}.png") } else { format!("{stem}{counter}.png") };
        match read_meta(&tx, "character", &id)? {
            Some(meta) if !meta.deleted => counter += 1,
            Some(meta) => break (id, meta.rev + 1),
            None => break (id, 1),
        }
    };
    let seq = store(&tx, "character", &id, rev, &hash, Some(&card.png))?;
    tx.commit()?;
    let created = json!({ "id": id, "name": card.name, "rev": rev, "seq": seq, "size": card.png.len() });
    Ok((StatusCode::CREATED, Json(created)).into_response())
}

pub fn router(server: Arc<Server>) -> Router {
    let api = Router::new()
        .route("/info", get(info))
        .route("/changes", get(changes))
        .route("/item", get(get_item).put(put_item).delete(delete_item))
        .route("/characters", post(import_character))
        .route_layer(middleware::from_fn_with_state(server.clone(), require_token));
    Router::new()
        .route("/api/v1/health", get(health))
        .nest("/api/v1", api)
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .with_state(server)
}

/// Serves until the listener fails; used by `main` and by the app's sync tests.
pub async fn serve(listener: tokio::net::TcpListener, server: Server) -> std::io::Result<()> {
    axum::serve(listener, router(Arc::new(server))).await
}
