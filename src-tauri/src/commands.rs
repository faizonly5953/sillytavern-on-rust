//! Tauri commands: the whole UI <-> Rust API surface (replaces the Express server).
//!
//! JS passes camelCase argument names (`charFile`); Tauri maps them to snake_case.
//! Binary payloads (images, card files) travel as base64 strings.

use std::collections::BTreeMap;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::character::{self, CharacterData, CharacterSummary};
use crate::chat::{self, ChatFile, ChatSummary};
use crate::cloud;
use crate::import_url;
use crate::error::{AppError, AppResult, invalid};
use crate::paths;
use crate::prompt::{self, BuiltPrompt, GenMode};
use crate::providers::{self, GenRequest, StreamEvent};
use crate::settings::{Provider, Settings, mask_secret};
use crate::state::{AppState, lock};
use crate::updates;

/// Runs file/image work on the blocking pool instead of the async runtime.
async fn blocking<T, F>(app: AppHandle, work: F) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce(&AppState) -> AppResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || work(&app.state::<AppState>()))
        .await
        .map_err(|e| invalid(format!("Background task failed: {e}")))?
}

/// Accepts plain base64 or a `data:...;base64,` URL.
fn decode_base64(data: &str) -> AppResult<Vec<u8>> {
    let payload = data.split_once("base64,").map_or(data, |(_, rest)| rest);
    B64.decode(payload.trim()).map_err(|e| invalid(format!("Invalid file data: {e}")))
}

fn decode_optional(data: Option<String>) -> AppResult<Option<Vec<u8>>> {
    data.filter(|d| !d.is_empty()).map(|d| decode_base64(&d)).transpose()
}

// ---------- settings & secrets ----------

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> AppResult<Settings> {
    Ok(lock(&state.settings).clone())
}

#[tauri::command]
pub async fn save_settings(state: State<'_, AppState>, settings: Settings) -> AppResult<()> {
    crate::settings::save_settings(&state.dirs.settings_file(), &settings)?;
    *lock(&state.settings) = settings;
    Ok(())
}

#[tauri::command]
pub async fn get_data_dir(state: State<'_, AppState>) -> AppResult<String> {
    Ok(state.dirs.root().display().to_string())
}

/// Masked key per provider (`null` when unset). Full keys never leave Rust.
#[tauri::command]
pub async fn get_secret_status(state: State<'_, AppState>) -> AppResult<BTreeMap<String, Option<String>>> {
    let secrets = lock(&state.secrets);
    Ok(Provider::ALL
        .iter()
        .map(|p| {
            let masked = secrets.get(p.secret_key()).filter(|v| !v.is_empty()).map(|v| mask_secret(v));
            (p.as_str().to_string(), masked)
        })
        .collect())
}

#[tauri::command]
pub async fn set_secret(state: State<'_, AppState>, provider: Provider, value: String) -> AppResult<()> {
    let mut secrets = lock(&state.secrets);
    let value = value.trim();
    if value.is_empty() {
        secrets.remove(provider.secret_key());
    } else {
        secrets.insert(provider.secret_key().to_string(), value.to_string());
    }
    crate::settings::save_secrets(&state.dirs.secrets_file(), &secrets)
}

#[tauri::command]
pub async fn set_persona_avatar(app: AppHandle, persona_id: String, image: String) -> AppResult<Settings> {
    let bytes = decode_base64(&image)?;
    blocking(app, move |s| {
        let png = character::normalize_avatar(&bytes, 256, 256)?;
        let file = format!("{}.png", paths::sanitize_file_stem(&persona_id));
        paths::write_atomic(&s.dirs.user_avatars().join(&file), &png)?;
        let mut settings = lock(&s.settings);
        let persona = settings
            .personas
            .iter_mut()
            .find(|p| p.id == persona_id)
            .ok_or_else(|| AppError::NotFound(format!("persona {persona_id}")))?;
        persona.avatar = Some(file);
        crate::settings::save_settings(&s.dirs.settings_file(), &settings)?;
        Ok(settings.clone())
    })
    .await
}

// ---------- characters ----------

#[tauri::command]
pub async fn list_characters(app: AppHandle) -> AppResult<Vec<CharacterSummary>> {
    blocking(app, |s| character::list(&s.dirs, &mut lock(&s.character_cache))).await
}

#[tauri::command]
pub async fn get_character(app: AppHandle, file: String) -> AppResult<CharacterData> {
    blocking(app, move |s| character::load(&s.dirs, &file)).await
}

#[tauri::command]
pub async fn create_character(app: AppHandle, card: CharacterData, avatar: Option<String>) -> AppResult<String> {
    let avatar = decode_optional(avatar)?;
    blocking(app, move |s| character::create(&s.dirs, &card, avatar.as_deref())).await
}

#[tauri::command]
pub async fn update_character(app: AppHandle, file: String, card: CharacterData, avatar: Option<String>) -> AppResult<()> {
    let avatar = decode_optional(avatar)?;
    blocking(app, move |s| character::update(&s.dirs, &file, &card, avatar.as_deref())).await
}

#[tauri::command]
pub async fn set_favorite(app: AppHandle, file: String, fav: bool) -> AppResult<()> {
    blocking(app, move |s| {
        let mut card = character::load(&s.dirs, &file)?;
        card.extensions.insert("fav".into(), Value::Bool(fav));
        character::update(&s.dirs, &file, &card, None)
    })
    .await
}

#[tauri::command]
pub async fn delete_character(app: AppHandle, file: String, delete_chats: bool) -> AppResult<()> {
    blocking(app, move |s| character::delete(&s.dirs, &file, delete_chats)).await
}

#[tauri::command]
pub async fn import_character(app: AppHandle, data: String, file_name: String) -> AppResult<String> {
    let bytes = decode_base64(&data)?;
    blocking(app, move |s| character::import(&s.dirs, &bytes, &file_name)).await
}

// ---------- chats ----------

#[tauri::command]
pub async fn list_chats(app: AppHandle, char_file: String) -> AppResult<Vec<ChatSummary>> {
    blocking(app, move |s| chat::list(&s.dirs, &char_file)).await
}

#[tauri::command]
pub async fn load_chat(app: AppHandle, char_file: String, chat_file: String) -> AppResult<ChatFile> {
    blocking(app, move |s| chat::load(&s.dirs, &char_file, &chat_file)).await
}

/// New chat opened with the character's greeting (alternate greetings as swipes).
#[tauri::command]
pub async fn create_chat(app: AppHandle, char_file: String) -> AppResult<ChatFile> {
    blocking(app, move |s| {
        let card = character::load(&s.dirs, &char_file)?;
        let persona = lock(&s.settings).persona();
        let ctx = prompt::macro_context(&card, &persona);
        let greetings = card.greetings().iter().map(|g| prompt::substitute_macros(g, &ctx)).collect();
        let header = chat::new_header(&persona.name, &card.name);
        let messages = chat::greeting_message(&card.name, greetings).into_iter().collect();
        chat::create(&s.dirs, &char_file, header, messages)
    })
    .await
}

#[tauri::command]
pub async fn save_chat(app: AppHandle, char_file: String, chat_file: String, header: Value, messages: Vec<Value>) -> AppResult<()> {
    blocking(app, move |s| chat::save(&s.dirs, &char_file, &chat_file, &header, &messages)).await
}

#[tauri::command]
pub async fn delete_chat(app: AppHandle, char_file: String, chat_file: String) -> AppResult<()> {
    blocking(app, move |s| chat::delete(&s.dirs, &char_file, &chat_file)).await
}

#[tauri::command]
pub async fn rename_chat(app: AppHandle, char_file: String, chat_file: String, new_name: String) -> AppResult<String> {
    blocking(app, move |s| chat::rename(&s.dirs, &char_file, &chat_file, &new_name)).await
}

// ---------- generation ----------

#[derive(Deserialize)]
pub struct GenerateRequest {
    pub gen_id: String,
    pub char_file: String,
    pub history: Vec<Value>,
    #[serde(default)]
    pub mode: GenMode,
}

#[derive(Serialize)]
pub struct GenerateOutcome {
    pub stopped: bool,
}

fn build_prompt(state: &AppState, char_file: &str, history: &[Value], mode: GenMode) -> AppResult<BuiltPrompt> {
    let card = character::load(&state.dirs, char_file)?;
    let settings = lock(&state.settings).clone();
    Ok(prompt::build(&card, &settings.persona(), &settings, history, mode))
}

/// Streams the reply through `on_event`; resolves when done or stopped.
#[tauri::command]
pub async fn generate(app: AppHandle, request: GenerateRequest, on_event: Channel<StreamEvent>) -> AppResult<GenerateOutcome> {
    let GenerateRequest { gen_id, char_file, history, mode } = request;
    let prompt = blocking(app.clone(), move |s| build_prompt(s, &char_file, &history, mode)).await?;
    let state = app.state::<AppState>();
    let settings = lock(&state.settings).clone();
    let api_key = lock(&state.secrets).get(settings.provider.secret_key()).cloned().unwrap_or_default();
    let client = state.http.clone();

    let task = tokio::spawn(async move {
        let request = GenRequest {
            conn: settings.connections.get(settings.provider),
            api_key: &api_key,
            params: &settings.generation,
            prompt: &prompt,
        };
        providers::generate(&client, settings.provider, request, |event| {
            let _ = on_event.send(event);
        })
        .await
    });
    lock(&state.generations).insert(gen_id.clone(), task.abort_handle());
    let result = task.await;
    lock(&state.generations).remove(&gen_id);

    match result {
        Ok(Ok(())) => Ok(GenerateOutcome { stopped: false }),
        Ok(Err(e)) => Err(e),
        Err(e) if e.is_cancelled() => Ok(GenerateOutcome { stopped: true }),
        Err(e) => Err(invalid(format!("Generation task failed: {e}"))),
    }
}

#[tauri::command]
pub async fn stop_generation(state: State<'_, AppState>, gen_id: String) -> AppResult<bool> {
    let handle = lock(&state.generations).remove(&gen_id);
    Ok(handle.map(|h| h.abort()).is_some())
}

#[tauri::command]
pub async fn preview_prompt(app: AppHandle, char_file: String, history: Vec<Value>, mode: Option<GenMode>) -> AppResult<BuiltPrompt> {
    blocking(app, move |s| build_prompt(s, &char_file, &history, mode.unwrap_or_default())).await
}

/// Downloads a card from Chub, JanitorAI, Pygmalion, AICC, RisuRealm or a direct link.
#[tauri::command]
pub async fn import_character_url(app: AppHandle, state: State<'_, AppState>, url: String) -> AppResult<String> {
    let downloaded = import_url::download(&state.http, &url).await?;
    blocking(app, move |s| match downloaded {
        import_url::Downloaded::File { bytes, name } => character::import(&s.dirs, &bytes, &name),
        import_url::Downloaded::Card { card, image } => character::import_card(&s.dirs, &card, image.as_deref()),
    })
    .await
}

// ---------- cloud sync ----------

#[derive(Deserialize)]
pub struct Resolution {
    pub key: String,
    pub choice: cloud::Choice,
}

#[tauri::command]
pub async fn cloud_status(state: State<'_, AppState>) -> AppResult<cloud::CloudStatus> {
    Ok(cloud::status(&state.dirs, &lock(&state.cloud)))
}

#[tauri::command]
pub async fn cloud_test(state: State<'_, AppState>, url: String, token: String) -> AppResult<cloud::ServerInfo> {
    cloud::test_connection(&state.http, &cloud::normalize_url(&url)?, &token).await
}

/// Verifies the server, then stores the connection for this device.
#[tauri::command]
pub async fn cloud_connect(state: State<'_, AppState>, url: String, token: String, passphrase: String) -> AppResult<cloud::CloudStatus> {
    let url = cloud::normalize_url(&url)?;
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err(invalid("The access token is required"));
    }
    cloud::test_connection(&state.http, &url, &token).await?;
    let previous_url = lock(&state.cloud).url.clone();
    if previous_url != url {
        cloud::reset_state(&state.dirs)?;
    }
    let config = cloud::CloudConfig { initialized: true, enabled: true, url, token, passphrase };
    cloud::save_config(&state.dirs, &config)?;
    *lock(&state.cloud) = config;
    Ok(cloud::status(&state.dirs, &lock(&state.cloud)))
}

/// First-run choice "use without a server", or turning sync off later. Local data is kept.
#[tauri::command]
pub async fn cloud_use_offline(state: State<'_, AppState>) -> AppResult<cloud::CloudStatus> {
    let config = cloud::CloudConfig { initialized: true, ..Default::default() };
    cloud::save_config(&state.dirs, &config)?;
    cloud::reset_state(&state.dirs)?;
    *lock(&state.cloud) = config;
    Ok(cloud::status(&state.dirs, &lock(&state.cloud)))
}

#[tauri::command]
pub async fn cloud_sync(state: State<'_, AppState>, resolutions: Vec<Resolution>) -> AppResult<cloud::SyncReport> {
    let choices = resolutions.into_iter().map(|r| (r.key, r.choice)).collect();
    cloud::sync(&state, &choices).await
}

// ---------- app & updates ----------

#[derive(Serialize)]
pub struct AppInfo {
    pub version: String,
    /// `windows`, `linux`, `macos`, `android`, `ios`.
    pub os: &'static str,
}

#[tauri::command]
pub async fn app_info(app: AppHandle) -> AppResult<AppInfo> {
    Ok(AppInfo { version: app.package_info().version.to_string(), os: std::env::consts::OS })
}

/// Latest published GitHub release if it is newer than this build, else `null`.
#[tauri::command]
pub async fn check_release(app: AppHandle, state: State<'_, AppState>) -> AppResult<Option<updates::ReleaseInfo>> {
    let current = app.package_info().version.to_string();
    updates::check_latest(&state.http, &current).await
}

#[tauri::command]
pub async fn list_models(state: State<'_, AppState>, provider: Provider) -> AppResult<Vec<String>> {
    let (conn, api_key, client) = {
        let settings = lock(&state.settings);
        let key = lock(&state.secrets).get(provider.secret_key()).cloned().unwrap_or_default();
        (settings.connections.get(provider).clone(), key, state.http.clone())
    };
    providers::list_models(&client, provider, &conn, &api_key).await
}
