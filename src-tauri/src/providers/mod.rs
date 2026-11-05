//! LLM provider dispatch (OpenAI-compatible, Claude, Gemini) and shared helpers.

pub mod claude;
pub mod gemini;
pub mod openai;
pub(crate) mod sse;

use futures_util::StreamExt;
use reqwest::header::CONTENT_TYPE;
use serde::Serialize;
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::prompt::{BuiltPrompt, Role};
use crate::settings::{GenerationParams, Provider, ProviderConn};

/// Streamed to the UI through a Tauri channel.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum StreamEvent {
    Delta { text: String },
    Reasoning { text: String },
}

pub struct GenRequest<'a> {
    pub conn: &'a ProviderConn,
    pub api_key: &'a str,
    pub params: &'a GenerationParams,
    pub prompt: &'a BuiltPrompt,
}

pub async fn generate<E: FnMut(StreamEvent) + Send>(
    client: &reqwest::Client,
    provider: Provider,
    req: GenRequest<'_>,
    mut emit: E,
) -> AppResult<()> {
    if req.conn.model.trim().is_empty() {
        return Err(AppError::Invalid("No model selected. Pick one in Settings \u{2192} Connection.".into()));
    }
    match provider {
        Provider::Openai => openai::generate(client, req, &mut emit).await,
        Provider::Claude => claude::generate(client, req, &mut emit).await,
        Provider::Gemini => gemini::generate(client, req, &mut emit).await,
    }
}

pub async fn list_models(client: &reqwest::Client, provider: Provider, conn: &ProviderConn, api_key: &str) -> AppResult<Vec<String>> {
    let mut models = match provider {
        Provider::Openai => openai::list_models(client, conn, api_key).await?,
        Provider::Claude => claude::list_models(client, conn, api_key).await?,
        Provider::Gemini => gemini::list_models(client, conn, api_key).await?,
    };
    models.sort();
    models.dedup();
    Ok(models)
}

pub(crate) fn base_url(conn: &ProviderConn, default: &str) -> String {
    let base = conn.base_url.trim().trim_end_matches('/');
    if base.is_empty() { default.to_string() } else { base.to_string() }
}

/// Turns a non-2xx response into a readable error, using the API's own message when present.
pub(crate) async fn check_status(resp: reqwest::Response) -> AppResult<reqwest::Response> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    let body = resp.text().await.unwrap_or_default();
    let detail = serde_json::from_str::<Value>(&body)
        .ok()
        .and_then(|v| {
            v.pointer("/error/message")
                .or_else(|| v.get("error"))
                .or_else(|| v.get("message"))
                .and_then(Value::as_str)
                .map(String::from)
        })
        .unwrap_or_else(|| body.chars().take(400).collect());
    Err(AppError::Api(format!("{} {}: {}", status.as_u16(), status.canonical_reason().unwrap_or(""), detail.trim())))
}

pub(crate) fn error_text(error: &Value) -> String {
    error.get("message").and_then(Value::as_str).map(String::from).unwrap_or_else(|| error.to_string())
}

/// Some local servers ignore `stream: true`; fall back to plain JSON when they do.
pub(crate) fn is_json_response(resp: &reqwest::Response) -> bool {
    resp.headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("application/json"))
}

/// Feeds each SSE event to `on_event` until it returns `Ok(false)` or the stream ends.
pub(crate) async fn read_sse(
    resp: reqwest::Response,
    mut on_event: impl FnMut(&sse::SseEvent) -> AppResult<bool>,
) -> AppResult<()> {
    let mut decoder = sse::SseDecoder::default();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        for event in decoder.push(&chunk?) {
            if !on_event(&event)? {
                return Ok(());
            }
        }
    }
    for event in decoder.finish() {
        if !on_event(&event)? {
            return Ok(());
        }
    }
    Ok(())
}

pub(crate) const START_MARKER: &str = "[Start a new Chat]";

/// For APIs with a separate system field (Claude, Gemini): leading system messages
/// become the system prompt, later ones become user turns, same-role turns merge,
/// and the conversation always opens with a user turn.
pub(crate) struct SplitPrompt {
    pub system: String,
    pub turns: Vec<(Role, String)>,
}

pub(crate) fn split_system(prompt: &BuiltPrompt) -> SplitPrompt {
    let leading = prompt.messages.iter().take_while(|m| m.role == Role::System).count();
    let system = prompt.messages[..leading].iter().map(|m| m.content.as_str()).collect::<Vec<_>>().join("\n\n");
    let mut turns: Vec<(Role, String)> = Vec::new();
    for m in &prompt.messages[leading..] {
        if m.content.trim().is_empty() {
            continue;
        }
        let role = if m.role == Role::Assistant { Role::Assistant } else { Role::User };
        match turns.last_mut() {
            Some((last_role, text)) if *last_role == role => {
                text.push_str("\n\n");
                text.push_str(&m.content);
            }
            _ => turns.push((role, m.content.clone())),
        }
    }
    if turns.first().map(|(role, _)| *role) != Some(Role::User) {
        turns.insert(0, (Role::User, START_MARKER.to_string()));
    }
    SplitPrompt { system, turns }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt::PromptMessage;

    fn msg(role: Role, content: &str) -> PromptMessage {
        PromptMessage { role, content: content.into(), tokens: 1 }
    }

    #[test]
    fn split_system_merges_and_opens_with_user() {
        let prompt = BuiltPrompt {
            messages: vec![
                msg(Role::System, "main"),
                msg(Role::System, "desc"),
                msg(Role::Assistant, "greeting"),
                msg(Role::User, "hi"),
                msg(Role::System, "post"),
            ],
            prefill: None,
            total_tokens: 0,
            budget: 0,
            history_included: 2,
            history_total: 2,
        };
        let split = split_system(&prompt);
        assert_eq!(split.system, "main\n\ndesc");
        assert_eq!(
            split.turns,
            vec![
                (Role::User, START_MARKER.to_string()),
                (Role::Assistant, "greeting".to_string()),
                (Role::User, "hi\n\npost".to_string()),
            ]
        );
    }
}
