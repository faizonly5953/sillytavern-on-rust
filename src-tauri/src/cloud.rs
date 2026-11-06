//! Cloud sync: local-first synchronisation with a self-hosted SillyTavern RS server.
//!
//! Local files stay the source of truth and the app works offline. A sync compares,
//! per item, the local content and the server revision against the `base` recorded
//! at the last successful sync:
//! - only local changed  -> upload (with the base revision, so the server can refuse)
//! - only cloud changed  -> download
//! - both changed        -> conflict; the user picks "local" or "cloud", then we sync again
//!
//! Items: character cards, chats, shared settings, persona avatars and API keys
//! (encrypted with the user's passphrase before they leave the device).
//! `cloud.json` (device config) and `sync-state.json` (bookkeeping) are never synced.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::crypto;
use crate::error::{AppError, AppResult, invalid};
use crate::paths::{self, DataDirs};
use crate::png_meta;
use crate::settings::{self, Connections, GenerationParams, Persona, PromptSettings, Provider, Secrets, Settings};
use crate::state::{AppState, lock};

// ---------- device configuration (never synced) ----------

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct CloudConfig {
    /// The user made the first-run choice (offline or a server).
    pub initialized: bool,
    pub enabled: bool,
    pub url: String,
    pub token: String,
    /// Encrypts API keys before upload. Empty means API keys are not synced.
    pub passphrase: String,
}

fn config_path(dirs: &DataDirs) -> PathBuf {
    dirs.root().join("cloud.json")
}

fn state_path(dirs: &DataDirs) -> PathBuf {
    dirs.root().join("sync-state.json")
}

pub fn load_config(dirs: &DataDirs) -> CloudConfig {
    fs::read_to_string(config_path(dirs)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save_config(dirs: &DataDirs, cfg: &CloudConfig) -> AppResult<()> {
    paths::write_atomic(&config_path(dirs), &serde_json::to_vec_pretty(cfg)?)
}

/// Forgets what was synced, so the next sync compares every item again.
pub fn reset_state(dirs: &DataDirs) -> AppResult<()> {
    let path = state_path(dirs);
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[derive(Serialize, Clone, Debug)]
pub struct CloudStatus {
    pub initialized: bool,
    pub enabled: bool,
    pub url: String,
    pub has_token: bool,
    pub has_passphrase: bool,
    pub last_sync: Option<i64>,
}

pub fn status(dirs: &DataDirs, cfg: &CloudConfig) -> CloudStatus {
    CloudStatus {
        initialized: cfg.initialized,
        enabled: cfg.enabled,
        url: cfg.url.clone(),
        has_token: !cfg.token.is_empty(),
        has_passphrase: !cfg.passphrase.is_empty(),
        last_sync: load_state(dirs).last_sync,
    }
}

/// Accepts `https://host[:port]`, `http://...` or a bare host (HTTPS assumed).
pub fn normalize_url(raw: &str) -> AppResult<String> {
    let trimmed = raw.trim().trim_end_matches('/');
    let url = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let host = url.split_once("://").map(|(_, h)| h).unwrap_or_default();
    if host.is_empty() || host.contains(char::is_whitespace) || host.contains('/') {
        return Err(invalid("Enter the server as host or host:port, for example sync.example.com"));
    }
    Ok(url)
}

// ---------- sync bookkeeping ----------

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct SyncState {
    server_id: String,
    last_seq: i64,
    last_sync: Option<i64>,
    base: BTreeMap<String, Base>,
    scan: BTreeMap<String, ScanEntry>,
}

/// What device and server agreed on for an item at its last successful sync.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct Base {
    rev: i64,
    hash: String,
    deleted: bool,
}

/// Cached content hash, reused while modification time and size are unchanged.
#[derive(Serialize, Deserialize, Clone)]
struct ScanEntry {
    modified: u64,
    size: u64,
    hash: String,
}

fn load_state(dirs: &DataDirs) -> SyncState {
    fs::read_to_string(state_path(dirs)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save_state(dirs: &DataDirs, state: &SyncState) -> AppResult<()> {
    paths::write_atomic(&state_path(dirs), &serde_json::to_vec(state)?)
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

// ---------- items ----------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Character,
    Chat,
    Settings,
    PersonaAvatar,
    Secrets,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Character => "character",
            Kind::Chat => "chat",
            Kind::Settings => "settings",
            Kind::PersonaAvatar => "persona_avatar",
            Kind::Secrets => "secrets",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "character" => Kind::Character,
            "chat" => Kind::Chat,
            "settings" => Kind::Settings,
            "persona_avatar" => Kind::PersonaAvatar,
            "secrets" => Kind::Secrets,
            _ => return None,
        })
    }
}

const SETTINGS_ID: &str = "settings";
const SECRETS_ID: &str = "secrets";

fn item_key(kind: Kind, id: &str) -> String {
    format!("{}:{id}", kind.as_str())
}

fn split_key(key: &str) -> Option<(Kind, &str)> {
    let (kind, id) = key.split_once(':')?;
    Some((Kind::parse(kind)?, id))
}

/// Local path of a file-backed item. Chat ids are `<character stem>/<chat file>`.
fn item_path(dirs: &DataDirs, kind: Kind, id: &str) -> AppResult<PathBuf> {
    match kind {
        Kind::Character => {
            let file = paths::check_file_name(id)?;
            if !file.ends_with(".png") {
                return Err(invalid(format!("Invalid character id: {id}")));
            }
            Ok(dirs.characters().join(file))
        }
        Kind::Chat => {
            let (stem, file) = id.split_once('/').ok_or_else(|| invalid(format!("Invalid chat id: {id}")))?;
            let (stem, file) = (paths::check_file_name(stem)?, paths::check_file_name(file)?);
            if !file.ends_with(".jsonl") {
                return Err(invalid(format!("Invalid chat id: {id}")));
            }
            Ok(dirs.chats_root().join(stem).join(file))
        }
        Kind::PersonaAvatar => Ok(dirs.user_avatars().join(paths::check_file_name(id)?)),
        Kind::Settings | Kind::Secrets => Err(invalid("Not a file item")),
    }
}

/// Settings shared between devices; UI preferences (theme, last chat...) stay per device.
#[derive(Serialize, Deserialize)]
struct SharedSettings {
    provider: Provider,
    connections: Connections,
    generation: GenerationParams,
    prompts: PromptSettings,
    personas: Vec<Persona>,
    active_persona: String,
}

fn shared_settings_bytes(s: &Settings) -> AppResult<Vec<u8>> {
    Ok(serde_json::to_vec(&SharedSettings {
        provider: s.provider,
        connections: s.connections.clone(),
        generation: s.generation.clone(),
        prompts: s.prompts.clone(),
        personas: s.personas.clone(),
        active_persona: s.active_persona.clone(),
    })?)
}

fn secrets_bytes(secrets: &Secrets) -> AppResult<Vec<u8>> {
    Ok(serde_json::to_vec(secrets)?)
}

struct LocalItem {
    hash: String,
    modified: u64,
    content: Content,
}

enum Content {
    File(PathBuf),
    Memory(Vec<u8>),
}

impl LocalItem {
    fn read(&self) -> AppResult<Vec<u8>> {
        match &self.content {
            Content::File(path) => Ok(fs::read(path)?),
            Content::Memory(bytes) => Ok(bytes.clone()),
        }
    }
}

fn list_files(dir: &std::path::Path, ext: &str) -> AppResult<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    if !dir.exists() {
        return Ok(out);
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if !path.is_file() {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|n| n.to_str())
            && name.ends_with(ext)
        {
            out.push((name.to_string(), path));
        }
    }
    Ok(out)
}

fn scan_local(state: &AppState, sync: &mut SyncState, include_secrets: bool) -> AppResult<BTreeMap<String, LocalItem>> {
    let dirs = &state.dirs;
    let mut files: Vec<(Kind, String, PathBuf)> = Vec::new();
    for (name, path) in list_files(&dirs.characters(), ".png")? {
        files.push((Kind::Character, name, path));
    }
    for entry in fs::read_dir(dirs.chats_root())? {
        let dir = entry?.path();
        let Some(stem) = dir.file_name().and_then(|n| n.to_str()).map(String::from) else { continue };
        if dir.is_dir() {
            for (name, path) in list_files(&dir, ".jsonl")? {
                files.push((Kind::Chat, format!("{stem}/{name}"), path));
            }
        }
    }
    for (name, path) in list_files(&dirs.user_avatars(), ".png")? {
        files.push((Kind::PersonaAvatar, name, path));
    }

    let mut out = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for (kind, id, path) in files {
        let key = item_key(kind, &id);
        let size = fs::metadata(&path)?.len();
        let modified = paths::modified_ms(&path);
        let hash = match sync.scan.get(&key) {
            Some(cached) if cached.modified == modified && cached.size == size => cached.hash.clone(),
            _ => {
                let hash = sha256_hex(&fs::read(&path)?);
                sync.scan.insert(key.clone(), ScanEntry { modified, size, hash: hash.clone() });
                hash
            }
        };
        seen.insert(key.clone());
        out.insert(key, LocalItem { hash, modified, content: Content::File(path) });
    }
    sync.scan.retain(|key, _| seen.contains(key));

    // Untouched default settings on a device that never synced them are not offered,
    // so a fresh device simply takes the cloud copy instead of reporting a conflict.
    let settings_key = item_key(Kind::Settings, SETTINGS_ID);
    let settings_bytes = shared_settings_bytes(&lock(&state.settings))?;
    let untouched = !sync.base.contains_key(&settings_key)
        && settings_bytes == shared_settings_bytes(&Settings::default())?;
    if !untouched {
        out.insert(
            settings_key,
            LocalItem {
                hash: sha256_hex(&settings_bytes),
                modified: paths::modified_ms(&dirs.settings_file()),
                content: Content::Memory(settings_bytes),
            },
        );
    }

    if include_secrets {
        let secrets = lock(&state.secrets).clone();
        if !secrets.is_empty() {
            let bytes = secrets_bytes(&secrets)?;
            out.insert(
                item_key(Kind::Secrets, SECRETS_ID),
                LocalItem {
                    hash: sha256_hex(&bytes),
                    modified: paths::modified_ms(&dirs.secrets_file()),
                    content: Content::Memory(bytes),
                },
            );
        }
    }
    Ok(out)
}

/// Writes downloaded content locally; returns the local content hash for the base.
fn apply_local(state: &AppState, cfg: &CloudConfig, kind: Kind, id: &str, data: &[u8]) -> AppResult<String> {
    match kind {
        Kind::Settings => {
            let shared: SharedSettings = serde_json::from_slice(data)?;
            let mut current = lock(&state.settings);
            current.provider = shared.provider;
            current.connections = shared.connections;
            current.generation = shared.generation;
            current.prompts = shared.prompts;
            current.personas = shared.personas;
            current.active_persona = shared.active_persona;
            settings::save_settings(&state.dirs.settings_file(), &current)?;
            Ok(sha256_hex(&shared_settings_bytes(&current)?))
        }
        Kind::Secrets => {
            let plain = crypto::decrypt(&cfg.passphrase, data)?;
            let secrets: Secrets = serde_json::from_slice(&plain)?;
            settings::save_secrets(&state.dirs.secrets_file(), &secrets)?;
            let hash = sha256_hex(&secrets_bytes(&secrets)?);
            *lock(&state.secrets) = secrets;
            Ok(hash)
        }
        Kind::Character | Kind::Chat | Kind::PersonaAvatar => {
            if kind != Kind::Chat && !png_meta::is_png(data) {
                return Err(invalid("Expected a PNG image"));
            }
            let path = item_path(&state.dirs, kind, id)?;
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            paths::write_atomic(&path, data)?;
            Ok(sha256_hex(data))
        }
    }
}

fn remove_local(state: &AppState, kind: Kind, id: &str) -> AppResult<()> {
    match kind {
        Kind::Settings => Ok(()),
        Kind::Secrets => {
            let mut secrets = lock(&state.secrets);
            secrets.clear();
            settings::save_secrets(&state.dirs.secrets_file(), &secrets)
        }
        Kind::Character | Kind::Chat | Kind::PersonaAvatar => {
            let path = item_path(&state.dirs, kind, id)?;
            if path.exists() {
                fs::remove_file(&path)?;
            }
            if kind == Kind::Chat
                && let Some(dir) = path.parent()
            {
                let _ = fs::remove_dir(dir); // only succeeds when the folder is empty
            }
            Ok(())
        }
    }
}

// ---------- server client ----------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ServerInfo {
    pub server_id: String,
    pub latest_seq: i64,
    pub version: String,
}

#[derive(Deserialize, Clone, Debug)]
struct RemoteMeta {
    kind: String,
    id: String,
    rev: i64,
    hash: String,
    deleted: bool,
    updated_at: i64,
}

#[derive(Deserialize)]
struct Changes {
    latest_seq: i64,
    items: Vec<RemoteMeta>,
}

enum WriteResult {
    Written(i64),
    Conflict(Option<RemoteMeta>),
}

struct Remote<'a> {
    http: &'a reqwest::Client,
    base: String,
    token: &'a str,
}

impl<'a> Remote<'a> {
    fn new(http: &'a reqwest::Client, cfg: &'a CloudConfig) -> AppResult<Self> {
        if cfg.token.trim().is_empty() {
            return Err(invalid("No access token configured for the cloud server"));
        }
        Ok(Self { http, base: normalize_url(&cfg.url)?, token: cfg.token.trim() })
    }

    fn request(&self, method: Method, path: &str) -> reqwest::RequestBuilder {
        self.http
            .request(method, format!("{}/api/v1/{path}", self.base))
            .bearer_auth(self.token)
            .timeout(Duration::from_secs(120))
    }

    async fn checked(resp: reqwest::Response) -> AppResult<reqwest::Response> {
        if resp.status() == StatusCode::UNAUTHORIZED {
            return Err(AppError::Api("The server rejected the access token".into()));
        }
        crate::providers::check_status(resp).await
    }

    async fn info(&self) -> AppResult<ServerInfo> {
        let resp = Self::checked(self.request(Method::GET, "info").send().await?).await?;
        Ok(resp.json().await?)
    }

    async fn changes(&self, since: i64) -> AppResult<Changes> {
        let resp = self.request(Method::GET, "changes").query(&[("since", since)]).send().await?;
        Ok(Self::checked(resp).await?.json().await?)
    }

    /// Returns (revision, deleted, bytes) of the current server copy.
    async fn get(&self, kind: Kind, id: &str) -> AppResult<(i64, bool, Vec<u8>)> {
        let resp = self.request(Method::GET, "item").query(&[("kind", kind.as_str()), ("id", id)]).send().await?;
        let resp = Self::checked(resp).await?;
        let header = |name: &str| resp.headers().get(name).and_then(|v| v.to_str().ok()).map(String::from);
        let rev = header("x-rev").and_then(|v| v.parse().ok()).unwrap_or(0);
        let deleted = header("x-deleted").is_some_and(|v| v == "1" || v == "true");
        Ok((rev, deleted, resp.bytes().await?.to_vec()))
    }

    /// Upload (`Some`) or delete (`None`), based on `base_rev`.
    async fn write(&self, kind: Kind, id: &str, base_rev: i64, body: Option<(Vec<u8>, String)>) -> AppResult<WriteResult> {
        let base_rev_text = base_rev.to_string();
        let request = match body {
            Some((bytes, hash)) => self
                .request(Method::PUT, "item")
                .query(&[("kind", kind.as_str()), ("id", id), ("base_rev", &base_rev_text), ("hash", &hash)])
                .body(bytes),
            None => self
                .request(Method::DELETE, "item")
                .query(&[("kind", kind.as_str()), ("id", id), ("base_rev", &base_rev_text)]),
        };
        let resp = request.send().await?;
        if resp.status() == StatusCode::CONFLICT {
            let body: Value = resp.json().await.unwrap_or_default();
            let current = body.get("current").cloned().and_then(|c| serde_json::from_value(c).ok());
            return Ok(WriteResult::Conflict(current));
        }
        let body: Value = Self::checked(resp).await?.json().await?;
        Ok(WriteResult::Written(body.get("rev").and_then(Value::as_i64).unwrap_or(base_rev + 1)))
    }
}

pub async fn test_connection(http: &reqwest::Client, url: &str, token: &str) -> AppResult<ServerInfo> {
    let cfg = CloudConfig { url: url.to_string(), token: token.to_string(), ..Default::default() };
    Remote::new(http, &cfg)?.info().await
}

// ---------- sync ----------

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Choice {
    Local,
    Cloud,
}

/// An item changed both here and on the server since the last sync.
#[derive(Serialize, Clone, Debug)]
pub struct Conflict {
    pub key: String,
    pub kind: String,
    pub id: String,
    pub local_exists: bool,
    pub cloud_exists: bool,
    pub local_modified: u64,
    pub cloud_modified: i64,
}

#[derive(Serialize, Default, Debug)]
pub struct SyncReport {
    pub pushed: usize,
    pub pulled: usize,
    /// Keys whose local copy was replaced or removed, so the UI can reload them.
    pub changed: Vec<String>,
    pub conflicts: Vec<Conflict>,
    pub errors: Vec<String>,
    pub last_sync: Option<i64>,
}

enum Plan {
    Push(i64),
    Pull,
}

enum Outcome {
    Pushed(Base),
    Pulled(Base),
    Conflict(Option<RemoteMeta>),
}

fn conflict(key: &str, kind: Kind, id: &str, local: Option<&LocalItem>, remote: Option<&RemoteMeta>) -> Conflict {
    Conflict {
        key: key.to_string(),
        kind: kind.as_str().to_string(),
        id: id.to_string(),
        local_exists: local.is_some(),
        cloud_exists: remote.is_some_and(|r| !r.deleted),
        local_modified: local.map_or(0, |l| l.modified),
        cloud_modified: remote.map_or(0, |r| r.updated_at),
    }
}

async fn push(remote: &Remote<'_>, cfg: &CloudConfig, kind: Kind, id: &str, local: Option<&LocalItem>, base_rev: i64) -> AppResult<Outcome> {
    let body = match local {
        None => None,
        Some(item) => {
            let bytes = item.read()?;
            Some(if kind == Kind::Secrets {
                let sealed = crypto::encrypt(&cfg.passphrase, &bytes)?;
                let hash = sha256_hex(&sealed);
                (sealed, hash)
            } else {
                (bytes, item.hash.clone())
            })
        }
    };
    Ok(match remote.write(kind, id, base_rev, body).await? {
        WriteResult::Written(rev) => Outcome::Pushed(Base {
            rev,
            hash: local.map(|l| l.hash.clone()).unwrap_or_default(),
            deleted: local.is_none(),
        }),
        WriteResult::Conflict(current) => Outcome::Conflict(current),
    })
}

async fn pull(remote: &Remote<'_>, state: &AppState, cfg: &CloudConfig, kind: Kind, id: &str) -> AppResult<Outcome> {
    let (rev, deleted, data) = remote.get(kind, id).await?;
    if deleted {
        remove_local(state, kind, id)?;
        return Ok(Outcome::Pulled(Base { rev, hash: String::new(), deleted: true }));
    }
    let hash = apply_local(state, cfg, kind, id, &data)?;
    Ok(Outcome::Pulled(Base { rev, hash, deleted: false }))
}

/// One sync round. `resolutions` answers conflicts reported by a previous round.
pub async fn sync(state: &AppState, resolutions: &HashMap<String, Choice>) -> AppResult<SyncReport> {
    let _running = state.sync_lock.lock().await;
    let cfg = lock(&state.cloud).clone();
    if !cfg.enabled {
        return Err(invalid("Cloud sync is turned off"));
    }
    let remote = Remote::new(&state.http, &cfg)?;
    let info = remote.info().await?;

    let mut sync = load_state(&state.dirs);
    if sync.server_id != info.server_id {
        // Different (or reset) server: compare everything from scratch.
        sync = SyncState { server_id: info.server_id.clone(), ..Default::default() };
    }
    let with_secrets = !cfg.passphrase.is_empty();
    let changes = remote.changes(sync.last_seq).await?;
    let remote_items: BTreeMap<String, RemoteMeta> = changes
        .items
        .into_iter()
        .filter_map(|meta| {
            let kind = Kind::parse(&meta.kind)?;
            (kind != Kind::Secrets || with_secrets).then(|| (item_key(kind, &meta.id), meta))
        })
        .collect();
    let local = scan_local(state, &mut sync, with_secrets)?;

    let mut keys: BTreeSet<String> = local.keys().chain(remote_items.keys()).cloned().collect();
    keys.extend(sync.base.keys().filter(|k| with_secrets || !k.starts_with("secrets:")).cloned());

    let mut report = SyncReport::default();
    for key in keys {
        let Some((kind, id)) = split_key(&key) else { continue };
        let id = id.to_string();
        let base = sync.base.get(&key).cloned();
        let local_item = local.get(&key);
        let remote_meta = remote_items.get(&key);
        let base_rev = base.as_ref().map_or(0, |b| b.rev);

        let local_changed = match (&base, local_item) {
            (None, None) => false,
            (None, Some(_)) => true,
            (Some(b), Some(l)) => b.deleted || b.hash != l.hash,
            (Some(b), None) => !b.deleted,
        };
        let remote_changed = remote_meta.is_some_and(|r| r.rev != base_rev);

        let plan = match (local_changed, remote_changed, remote_meta) {
            (false, false, _) => continue,
            (true, false, _) => Plan::Push(base_rev),
            (false, true, _) => Plan::Pull,
            (true, true, Some(r)) => {
                let identical = match local_item {
                    Some(l) => !r.deleted && kind != Kind::Secrets && l.hash == r.hash,
                    None => r.deleted,
                };
                if identical {
                    let hash = local_item.map(|l| l.hash.clone()).unwrap_or_default();
                    sync.base.insert(key.clone(), Base { rev: r.rev, hash, deleted: r.deleted });
                    continue;
                }
                match resolutions.get(&key) {
                    Some(Choice::Local) => Plan::Push(r.rev),
                    Some(Choice::Cloud) => Plan::Pull,
                    None => {
                        report.conflicts.push(conflict(&key, kind, &id, local_item, Some(r)));
                        continue;
                    }
                }
            }
            (true, true, None) => unreachable!("remote_changed implies remote metadata"),
        };

        let outcome = match plan {
            Plan::Push(rev) => push(&remote, &cfg, kind, &id, local_item, rev).await,
            Plan::Pull => pull(&remote, state, &cfg, kind, &id).await,
        };
        match outcome {
            Ok(Outcome::Pushed(new_base)) => {
                sync.base.insert(key, new_base);
                report.pushed += 1;
            }
            Ok(Outcome::Pulled(new_base)) => {
                sync.base.insert(key.clone(), new_base);
                report.pulled += 1;
                report.changed.push(key);
            }
            Ok(Outcome::Conflict(current)) => {
                report.conflicts.push(conflict(&key, kind, &id, local_item, current.as_ref()));
            }
            Err(e) => report.errors.push(format!("{key}: {e}")),
        }
    }

    // Keep re-reading the same changes until every conflict and error is settled.
    if report.conflicts.is_empty() && report.errors.is_empty() {
        sync.last_seq = sync.last_seq.max(changes.latest_seq);
    }
    let now = now_ms();
    sync.last_sync = Some(now);
    report.last_sync = Some(now);
    save_state(&state.dirs, &sync)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::{self, CharacterData};
    use crate::chat;
    use serde_json::json;

    #[test]
    fn urls_are_normalized() {
        assert_eq!(normalize_url("sync.example.com/").unwrap(), "https://sync.example.com");
        assert_eq!(normalize_url("http://10.0.0.2:8787").unwrap(), "http://10.0.0.2:8787");
        assert!(normalize_url("  ").is_err());
        assert!(normalize_url("https://host/path").is_err());
    }

    async fn start_server() -> String {
        let dir = tempfile::tempdir().unwrap().keep();
        let (server, _) = sillytavern_rs_server::Server::open(&dir, Some("test-token".into())).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(sillytavern_rs_server::serve(listener, server));
        format!("http://{addr}")
    }

    fn device(url: &str) -> (tempfile::TempDir, AppState) {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::new(dir.path().to_path_buf()).unwrap();
        *lock(&state.cloud) = CloudConfig {
            initialized: true,
            enabled: true,
            url: url.into(),
            token: "test-token".into(),
            passphrase: "shared passphrase".into(),
        };
        (dir, state)
    }

    fn resolve(key: &str, choice: Choice) -> HashMap<String, Choice> {
        HashMap::from([(key.to_string(), choice)])
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn two_devices_sync_resolve_and_delete() {
        let url = start_server().await;
        let (_a_dir, a) = device(&url);
        let (_b_dir, b) = device(&url);
        let none = HashMap::new();

        // Device A creates data.
        let card = CharacterData { name: "Ann".into(), first_mes: "Hi".into(), ..Default::default() };
        let file = character::create(&a.dirs, &card, None).unwrap();
        let stem = file.trim_end_matches(".png").to_string();
        let msg = |text: &str| json!({ "name": "U", "is_user": true, "mes": text });
        let created = chat::create(&a.dirs, &file, chat::new_header("U", "Ann"), vec![msg("hello")]).unwrap();
        let chat_key = format!("chat:{stem}/{}", created.file);
        lock(&a.secrets).insert("api_key_openai".into(), "sk-secret".into());
        lock(&a.settings).generation.temperature = 0.42;
        let report = sync(&a, &none).await.unwrap();
        assert!(report.conflicts.is_empty() && report.errors.is_empty(), "{report:?}");
        assert_eq!(report.pushed, 4); // character, chat, settings, secrets

        // Device B (fresh, default settings) receives everything without conflicts.
        let report = sync(&b, &none).await.unwrap();
        assert!(report.conflicts.is_empty() && report.errors.is_empty(), "{report:?}");
        assert_eq!(report.pulled, 4);
        assert_eq!(character::load(&b.dirs, &file).unwrap().name, "Ann");
        assert_eq!(lock(&b.secrets).get("api_key_openai").map(String::as_str), Some("sk-secret"));
        assert_eq!(lock(&b.settings).generation.temperature, 0.42);

        // Nothing changed: a second round does nothing.
        let report = sync(&b, &none).await.unwrap();
        assert_eq!((report.pushed, report.pulled), (0, 0));

        // Both edit the same chat -> conflict on B; B keeps its local copy.
        chat::save(&a.dirs, &file, &created.file, &created.header, &[msg("from A")]).unwrap();
        assert_eq!(sync(&a, &none).await.unwrap().pushed, 1);
        chat::save(&b.dirs, &file, &created.file, &created.header, &[msg("from B")]).unwrap();
        let report = sync(&b, &none).await.unwrap();
        assert_eq!(report.conflicts.len(), 1);
        assert_eq!(report.conflicts[0].key, chat_key);
        let report = sync(&b, &resolve(&chat_key, Choice::Local)).await.unwrap();
        assert!(report.conflicts.is_empty() && report.pushed == 1, "{report:?}");
        let report = sync(&a, &none).await.unwrap();
        assert!(report.changed.contains(&chat_key));
        assert_eq!(chat::load(&a.dirs, &file, &created.file).unwrap().messages[0]["mes"], "from B");

        // Conflict resolved the other way: A takes the cloud copy.
        chat::save(&b.dirs, &file, &created.file, &created.header, &[msg("B again")]).unwrap();
        sync(&b, &none).await.unwrap();
        chat::save(&a.dirs, &file, &created.file, &created.header, &[msg("A again")]).unwrap();
        assert_eq!(sync(&a, &none).await.unwrap().conflicts.len(), 1);
        sync(&a, &resolve(&chat_key, Choice::Cloud)).await.unwrap();
        assert_eq!(chat::load(&a.dirs, &file, &created.file).unwrap().messages[0]["mes"], "B again");

        // Deleting the character (and its chats) on A removes them on B.
        character::delete(&a.dirs, &file, true).unwrap();
        sync(&a, &none).await.unwrap();
        let report = sync(&b, &none).await.unwrap();
        assert!(report.errors.is_empty(), "{report:?}");
        assert!(character::load(&b.dirs, &file).is_err());
        assert!(chat::load(&b.dirs, &file, &created.file).is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn bad_token_and_wrong_passphrase_are_reported() {
        let url = start_server().await;
        let (_dir, a) = device(&url);
        lock(&a.cloud).token = "nope".into();
        let err = sync(&a, &HashMap::new()).await.unwrap_err().to_string();
        assert!(err.contains("access token"), "{err}");

        let (_a_dir, a) = device(&url);
        lock(&a.secrets).insert("api_key_claude".into(), "sk-ant".into());
        sync(&a, &HashMap::new()).await.unwrap();
        let (_b_dir, b) = device(&url);
        lock(&b.cloud).passphrase = "different".into();
        let report = sync(&b, &HashMap::new()).await.unwrap();
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].contains("Wrong passphrase"));
        assert!(lock(&b.secrets).is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn persona_aliases_sync_in_both_directions() {
        let url = start_server().await;
        let (_a_dir, a) = device(&url);
        let (_b_dir, b) = device(&url);
        let none = HashMap::new();
        let persona = |id: &str, alias: &str| Persona { id: id.into(), name: "Wren".into(), alias: alias.into(), ..Default::default() };
        let aliases = |d: &AppState| lock(&d.settings).personas.iter().map(|p| p.alias.clone()).collect::<Vec<_>>();

        // Two personas with the same name, told apart by their alias.
        {
            let mut settings = lock(&a.settings);
            settings.personas = vec![persona("a", "Wren (fantasy)"), persona("b", "Wren (modern day)")];
            settings.active_persona = "b".into();
        }
        sync(&a, &none).await.unwrap();
        sync(&b, &none).await.unwrap();
        assert_eq!(aliases(&b), ["Wren (fantasy)", "Wren (modern day)"]);

        // Changing an alias on B reaches A.
        lock(&b.settings).personas[0].alias = "Wren (high fantasy)".into();
        assert_eq!(sync(&b, &none).await.unwrap().pushed, 1);
        sync(&a, &none).await.unwrap();
        assert_eq!(aliases(&a), ["Wren (high fantasy)", "Wren (modern day)"]);
    }

    async fn post_card(url: &str, token: &str, body: Vec<u8>) -> (StatusCode, Value) {
        let resp = reqwest::Client::new()
            .post(format!("{url}/api/v1/characters"))
            .bearer_auth(token)
            .body(body)
            .send()
            .await
            .unwrap();
        (resp.status(), resp.json().await.unwrap_or(Value::Null))
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cards_posted_to_the_server_reach_devices() {
        let url = start_server().await;
        let (_dir, a) = device(&url);
        let none = HashMap::new();
        let created = |(status, body): (StatusCode, Value)| {
            assert_eq!(status, StatusCode::CREATED, "{body}");
            body["id"].as_str().unwrap().to_string()
        };

        // A PNG card keeps its artwork; JSON cards (V1, Pygmalion, V3) get a generated avatar.
        let card = CharacterData { name: "Mira".into(), first_mes: "Hi".into(), ..Default::default() };
        let png = character::write_png_card(&character::default_avatar("Mira").unwrap(), &card).unwrap();
        assert_eq!(created(post_card(&url, "test-token", png.clone()).await), "Mira.png");
        let second = br#"{"name":"Mira","description":"second"}"#.to_vec();
        assert_eq!(created(post_card(&url, "test-token", second).await), "Mira1.png"); // never overwrites
        let pyg = br#"{"char_name":"Pyg","char_persona":"p","char_greeting":"g"}"#.to_vec();
        assert_eq!(created(post_card(&url, "test-token", pyg).await), "Pyg.png");
        let v3 = br#"{"spec":"chara_card_v3","spec_version":"3.0","data":{"name":"Vee","first_mes":"yo"}}"#.to_vec();
        assert_eq!(created(post_card(&url, "test-token", v3).await), "Vee.png");
        let odd = br#"{"name":"A/B: C?"}"#.to_vec();
        assert_eq!(created(post_card(&url, "test-token", odd).await), "A_B_ C_.png");

        // Rejected: wrong token, nothing usable, a plain image, a cut-off PNG, no name.
        assert_eq!(post_card(&url, "wrong", png.clone()).await.0, StatusCode::UNAUTHORIZED);
        let bad_bodies = [
            b"hello".to_vec(),
            Vec::new(),
            character::default_avatar("Plain").unwrap(),
            png[..png.len() - 20].to_vec(),
            br#"{"description":"no name"}"#.to_vec(),
        ];
        for bad in bad_bodies {
            let (status, body) = post_card(&url, "test-token", bad).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
            assert!(body["error"].is_string());
        }

        // The device receives all five, and the app reads them like cards it imported itself.
        let report = sync(&a, &none).await.unwrap();
        assert!(report.conflicts.is_empty() && report.errors.is_empty(), "{report:?}");
        assert_eq!(report.pulled, 5);
        assert_eq!(character::load(&a.dirs, "Mira.png").unwrap().first_mes, "Hi");
        assert_eq!(character::load(&a.dirs, "Mira1.png").unwrap().description, "second");
        let pyg = character::load(&a.dirs, "Pyg.png").unwrap();
        assert_eq!((pyg.description.as_str(), pyg.first_mes.as_str()), ("p", "g"));
        assert_eq!(character::load(&a.dirs, "Vee.png").unwrap().first_mes, "yo");
        assert_eq!(character::load(&a.dirs, "A_B_ C_.png").unwrap().name, "A/B: C?");
        assert_eq!(character::list(&a.dirs, &mut HashMap::new()).unwrap().len(), 5);
        // The generated avatar is a real image the app can decode and resize.
        let avatar = image::load_from_memory(&fs::read(a.dirs.characters().join("Pyg.png")).unwrap()).unwrap();
        assert_eq!((avatar.width(), avatar.height()), (64, 96));

        // A deleted card's name can be used again.
        character::delete(&a.dirs, "Mira.png", false).unwrap();
        sync(&a, &none).await.unwrap();
        let again = br#"{"name":"Mira","first_mes":"back again"}"#.to_vec();
        assert_eq!(created(post_card(&url, "test-token", again).await), "Mira.png");
        let report = sync(&a, &none).await.unwrap();
        assert!(report.errors.is_empty() && report.conflicts.is_empty(), "{report:?}");
        assert_eq!(character::load(&a.dirs, "Mira.png").unwrap().first_mes, "back again");
    }
}
