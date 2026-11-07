//! Anthropic Messages API.

use serde_json::{Value, json};

use super::{GenRequest, StreamEvent, base_url, check_status, error_text, is_json_response, read_sse, split_system};
use crate::error::{AppError, AppResult};
use crate::prompt::Role;
use crate::settings::ProviderConn;

const DEFAULT_BASE: &str = "https://api.anthropic.com/v1";
const API_VERSION: &str = "2023-06-01";

pub(crate) fn request_body(req: &GenRequest<'_>) -> Value {
    let split = split_system(req.prompt);
    let mut messages: Vec<Value> = split
        .turns
        .iter()
        .map(|(role, text)| json!({ "role": if *role == Role::Assistant { "assistant" } else { "user" }, "content": text }))
        .collect();
    if let Some(prefill) = req.prompt.prefill.as_deref().filter(|p| !p.trim().is_empty()) {
        messages.push(json!({ "role": "assistant", "content": prefill }));
    }
    // A final assistant turn is a prefill; the API rejects trailing whitespace there.
    if let Some(last) = messages.last_mut().filter(|m| m["role"] == "assistant") {
        let trimmed = last["content"].as_str().unwrap_or_default().trim_end().to_string();
        last["content"] = json!(trimmed);
    }
    let params = req.params;
    let mut body = json!({
        "model": req.conn.model.trim(),
        "max_tokens": params.max_tokens,
        "messages": messages,
        "stream": params.stream,
        // Claude accepts 0..=1; sending top_p alongside temperature is rejected by newer models.
        "temperature": params.temperature.clamp(0.0, 1.0),
    });
    if !split.system.is_empty() {
        body["system"] = json!(split.system);
    }
    if params.top_k > 0 {
        body["top_k"] = json!(params.top_k);
    }
    body
}

fn emit_block<E: FnMut(StreamEvent)>(block: &Value, emit: &mut E) {
    let text = |key: &str| block.get(key).and_then(Value::as_str).filter(|t| !t.is_empty()).map(String::from);
    match block.get("type").and_then(Value::as_str) {
        Some("text" | "text_delta") => {
            if let Some(text) = text("text") {
                emit(StreamEvent::Delta { text });
            }
        }
        Some("thinking" | "thinking_delta") => {
            if let Some(text) = text("thinking") {
                emit(StreamEvent::Reasoning { text });
            }
        }
        _ => {}
    }
}

fn request(client: &reqwest::Client, url: String, api_key: &str) -> reqwest::RequestBuilder {
    client.post(url).header("x-api-key", api_key).header("anthropic-version", API_VERSION)
}

pub async fn generate<E: FnMut(StreamEvent) + Send>(client: &reqwest::Client, req: GenRequest<'_>, emit: &mut E) -> AppResult<()> {
    if req.api_key.is_empty() {
        return Err(AppError::Invalid("No Claude API key. Add it in Settings \u{2192} Connection.".into()));
    }
    let base = base_url(req.conn, DEFAULT_BASE);
    let body = request_body(&req);
    let resp = check_status(request(client, format!("{base}/messages"), req.api_key).json(&body).send().await?).await?;

    if req.params.stream && !is_json_response(&resp) {
        return read_sse(resp, |event| {
            let Ok(data) = serde_json::from_str::<Value>(&event.data) else { return Ok(true) };
            match data.get("type").and_then(Value::as_str) {
                Some("content_block_delta") => {
                    if let Some(delta) = data.get("delta") {
                        emit_block(delta, emit);
                    }
                    Ok(true)
                }
                Some("error") => Err(AppError::Api(data.get("error").map(error_text).unwrap_or_else(|| data.to_string()))),
                Some("message_stop") => Ok(false),
                _ => Ok(true),
            }
        })
        .await;
    }

    let full: Value = resp.json().await?;
    for block in full.get("content").and_then(Value::as_array).into_iter().flatten() {
        emit_block(block, emit);
    }
    Ok(())
}

pub async fn list_models(client: &reqwest::Client, conn: &ProviderConn, api_key: &str) -> AppResult<Vec<String>> {
    let base = base_url(conn, DEFAULT_BASE);
    let resp = client
        .get(format!("{base}/models?limit=1000"))
        .header("x-api-key", api_key)
        .header("anthropic-version", API_VERSION)
        .send()
        .await?;
    let body: Value = check_status(resp).await?.json().await?;
    Ok(body
        .get("data")
        .and_then(Value::as_array)
        .map(|models| models.iter().filter_map(|m| m.get("id").and_then(Value::as_str).map(String::from)).collect())
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt::{BuiltPrompt, PromptMessage};
    use crate::settings::GenerationParams;

    #[test]
    fn system_prefill_and_temperature() {
        let conn = ProviderConn { base_url: String::new(), model: "claude-test".into() };
        let params = GenerationParams { temperature: 1.6, ..Default::default() };
        let prompt = BuiltPrompt {
            messages: vec![
                PromptMessage { role: Role::System, content: "sys".into(), tokens: 1 },
                PromptMessage { role: Role::User, content: "hi".into(), tokens: 1 },
            ],
            prefill: Some("Sure, ".into()),
            total_tokens: 2,
            tokens_exact: false,
            budget: 10,
            history_included: 1,
            history_total: 1,
        };
        let body = request_body(&GenRequest { conn: &conn, api_key: "k", params: &params, prompt: &prompt });
        assert_eq!(body["system"], "sys");
        assert_eq!(body["temperature"], 1.0);
        assert_eq!(body["messages"][1], json!({"role": "assistant", "content": "Sure,"}));
        assert!(body.get("top_p").is_none());
    }
}
