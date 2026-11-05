//! OpenAI-compatible Chat Completions (OpenAI, OpenRouter, DeepSeek, local servers).

use serde_json::{Value, json};

use super::{GenRequest, StreamEvent, base_url, check_status, error_text, is_json_response, read_sse};
use crate::error::{AppError, AppResult};
use crate::prompt::Role;
use crate::settings::ProviderConn;

const DEFAULT_BASE: &str = "https://api.openai.com/v1";

fn role_name(role: Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
    }
}

pub(crate) fn request_body(req: &GenRequest<'_>, official: bool) -> Value {
    let params = req.params;
    let messages: Vec<Value> = req
        .prompt
        .messages
        .iter()
        .map(|m| json!({ "role": role_name(m.role), "content": m.content }))
        .collect();
    let mut body = json!({
        "model": req.conn.model.trim(),
        "messages": messages,
        "stream": params.stream,
        "temperature": params.temperature,
        "top_p": params.top_p,
        "frequency_penalty": params.frequency_penalty,
        "presence_penalty": params.presence_penalty,
    });
    if official {
        // Current OpenAI models reject `max_tokens` and unknown sampler fields.
        body["max_completion_tokens"] = json!(params.max_tokens);
    } else {
        body["max_tokens"] = json!(params.max_tokens);
        if params.top_k > 0 {
            body["top_k"] = json!(params.top_k);
        }
    }
    body
}

fn emit_parts<E: FnMut(StreamEvent)>(part: &Value, emit: &mut E) {
    let reasoning = ["reasoning_content", "reasoning"]
        .iter()
        .find_map(|key| part.get(*key).and_then(Value::as_str).filter(|t| !t.is_empty()));
    if let Some(text) = reasoning {
        emit(StreamEvent::Reasoning { text: text.to_string() });
    }
    if let Some(text) = part.get("content").and_then(Value::as_str).filter(|t| !t.is_empty()) {
        emit(StreamEvent::Delta { text: text.to_string() });
    }
}

pub async fn generate<E: FnMut(StreamEvent) + Send>(client: &reqwest::Client, req: GenRequest<'_>, emit: &mut E) -> AppResult<()> {
    let base = base_url(req.conn, DEFAULT_BASE);
    let body = request_body(&req, base.contains("api.openai.com"));
    let mut request = client.post(format!("{base}/chat/completions")).json(&body);
    if !req.api_key.is_empty() {
        request = request.bearer_auth(req.api_key);
    }
    let resp = check_status(request.send().await?).await?;

    if req.params.stream && !is_json_response(&resp) {
        return read_sse(resp, |event| {
            if event.data.trim() == "[DONE]" {
                return Ok(false);
            }
            let Ok(chunk) = serde_json::from_str::<Value>(&event.data) else { return Ok(true) };
            if let Some(error) = chunk.get("error") {
                return Err(AppError::Api(error_text(error)));
            }
            if let Some(delta) = chunk.pointer("/choices/0/delta") {
                emit_parts(delta, emit);
            }
            Ok(true)
        })
        .await;
    }

    let full: Value = resp.json().await?;
    if let Some(error) = full.get("error") {
        return Err(AppError::Api(error_text(error)));
    }
    let message = full
        .pointer("/choices/0/message")
        .ok_or_else(|| AppError::Api("The API returned no choices".into()))?;
    emit_parts(message, emit);
    Ok(())
}

pub async fn list_models(client: &reqwest::Client, conn: &ProviderConn, api_key: &str) -> AppResult<Vec<String>> {
    let base = base_url(conn, DEFAULT_BASE);
    let mut request = client.get(format!("{base}/models"));
    if !api_key.is_empty() {
        request = request.bearer_auth(api_key);
    }
    let body: Value = check_status(request.send().await?).await?.json().await?;
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
    fn body_differs_for_official_api() {
        let conn = ProviderConn { base_url: String::new(), model: " gpt-x ".into() };
        let params = GenerationParams { top_k: 40, ..Default::default() };
        let prompt = BuiltPrompt {
            messages: vec![PromptMessage { role: Role::User, content: "hi".into(), tokens: 1 }],
            prefill: None,
            total_tokens: 1,
            budget: 1,
            history_included: 1,
            history_total: 1,
        };
        let req = GenRequest { conn: &conn, api_key: "", params: &params, prompt: &prompt };
        let official = request_body(&req, true);
        assert_eq!(official["model"], "gpt-x");
        assert!(official.get("max_tokens").is_none() && official.get("top_k").is_none());
        let local = request_body(&req, false);
        assert_eq!((local["max_tokens"].as_u64(), local["top_k"].as_u64()), (Some(400), Some(40)));
    }

    #[test]
    fn reasoning_and_content_are_split() {
        let mut events = Vec::new();
        emit_parts(&json!({"reasoning_content": "think", "content": "say"}), &mut |e| events.push(e));
        assert_eq!(
            events,
            vec![StreamEvent::Reasoning { text: "think".into() }, StreamEvent::Delta { text: "say".into() }]
        );
    }
}
