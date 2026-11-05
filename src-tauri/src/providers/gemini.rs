//! Google Gemini (Generative Language API).

use serde_json::{Value, json};

use super::{GenRequest, StreamEvent, base_url, check_status, is_json_response, read_sse, split_system};
use crate::error::{AppError, AppResult};
use crate::prompt::Role;
use crate::settings::ProviderConn;

const DEFAULT_BASE: &str = "https://generativelanguage.googleapis.com/v1beta";
const SAFETY_CATEGORIES: [&str; 4] = [
    "HARM_CATEGORY_HARASSMENT",
    "HARM_CATEGORY_HATE_SPEECH",
    "HARM_CATEGORY_SEXUALLY_EXPLICIT",
    "HARM_CATEGORY_DANGEROUS_CONTENT",
];
const BLOCKING_FINISH: [&str; 4] = ["SAFETY", "PROHIBITED_CONTENT", "BLOCKLIST", "RECITATION"];

/// Accepts both `gemini-x` and `models/gemini-x`.
fn model_path(model: &str) -> String {
    let model = model.trim();
    format!("models/{}", model.strip_prefix("models/").unwrap_or(model))
}

pub(crate) fn request_body(req: &GenRequest<'_>) -> Value {
    let split = split_system(req.prompt);
    let contents: Vec<Value> = split
        .turns
        .iter()
        .map(|(role, text)| json!({ "role": if *role == Role::Assistant { "model" } else { "user" }, "parts": [{ "text": text }] }))
        .collect();
    let params = req.params;
    let mut config = json!({
        "temperature": params.temperature,
        "topP": params.top_p,
        "maxOutputTokens": params.max_tokens,
        "candidateCount": 1,
    });
    if params.top_k > 0 {
        config["topK"] = json!(params.top_k);
    }
    let safety: Vec<Value> = SAFETY_CATEGORIES.iter().map(|c| json!({ "category": c, "threshold": "BLOCK_NONE" })).collect();
    let mut body = json!({ "contents": contents, "generationConfig": config, "safetySettings": safety });
    if !split.system.is_empty() {
        body["systemInstruction"] = json!({ "parts": [{ "text": split.system }] });
    }
    body
}

/// Emits a response chunk; tracks whether any visible text arrived so a silent
/// safety stop becomes an error instead of an empty reply.
pub(crate) fn handle_chunk<E: FnMut(StreamEvent)>(chunk: &Value, got_text: &mut bool, emit: &mut E) -> AppResult<()> {
    if let Some(reason) = chunk.pointer("/promptFeedback/blockReason").and_then(Value::as_str) {
        return Err(AppError::Api(format!("Gemini blocked the prompt ({reason})")));
    }
    let candidate = chunk.pointer("/candidates/0");
    for part in candidate.and_then(|c| c.pointer("/content/parts")).and_then(Value::as_array).into_iter().flatten() {
        let Some(text) = part.get("text").and_then(Value::as_str).filter(|t| !t.is_empty()) else { continue };
        if part.get("thought").and_then(Value::as_bool).unwrap_or(false) {
            emit(StreamEvent::Reasoning { text: text.to_string() });
        } else {
            *got_text = true;
            emit(StreamEvent::Delta { text: text.to_string() });
        }
    }
    let finish = candidate.and_then(|c| c.get("finishReason")).and_then(Value::as_str);
    if let Some(reason) = finish.filter(|r| BLOCKING_FINISH.contains(r))
        && !*got_text
    {
        return Err(AppError::Api(format!("Gemini stopped without a reply (finishReason: {reason})")));
    }
    Ok(())
}

pub async fn generate<E: FnMut(StreamEvent) + Send>(client: &reqwest::Client, req: GenRequest<'_>, emit: &mut E) -> AppResult<()> {
    if req.api_key.is_empty() {
        return Err(AppError::Invalid("No Gemini API key. Add it in Settings \u{2192} Connection.".into()));
    }
    let base = base_url(req.conn, DEFAULT_BASE);
    let model = model_path(&req.conn.model);
    let url = if req.params.stream {
        format!("{base}/{model}:streamGenerateContent?alt=sse")
    } else {
        format!("{base}/{model}:generateContent")
    };
    let body = request_body(&req);
    let resp = check_status(client.post(url).header("x-goog-api-key", req.api_key).json(&body).send().await?).await?;

    let mut got_text = false;
    if req.params.stream && !is_json_response(&resp) {
        read_sse(resp, |event| {
            if let Ok(chunk) = serde_json::from_str::<Value>(&event.data) {
                handle_chunk(&chunk, &mut got_text, emit)?;
            }
            Ok(true)
        })
        .await?;
    } else {
        let full: Value = resp.json().await?;
        handle_chunk(&full, &mut got_text, emit)?;
    }
    Ok(())
}

pub async fn list_models(client: &reqwest::Client, conn: &ProviderConn, api_key: &str) -> AppResult<Vec<String>> {
    let base = base_url(conn, DEFAULT_BASE);
    let resp = client.get(format!("{base}/models?pageSize=1000")).header("x-goog-api-key", api_key).send().await?;
    let body: Value = check_status(resp).await?.json().await?;
    let supports_chat = |m: &Value| {
        m.get("supportedGenerationMethods")
            .and_then(Value::as_array)
            .is_some_and(|methods| methods.iter().any(|x| x == "generateContent"))
    };
    Ok(body
        .get("models")
        .and_then(Value::as_array)
        .map(|models| {
            models
                .iter()
                .filter(|m| supports_chat(m))
                .filter_map(|m| m.get("name").and_then(Value::as_str))
                .map(|name| name.strip_prefix("models/").unwrap_or(name).to_string())
                .collect()
        })
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_prefix_is_normalized() {
        assert_eq!(model_path("models/gemini-x"), "models/gemini-x");
        assert_eq!(model_path(" gemini-x "), "models/gemini-x");
    }

    #[test]
    fn chunks_split_thoughts_and_detect_blocks() {
        let mut events = Vec::new();
        let mut got = false;
        let chunk = json!({"candidates":[{"content":{"parts":[{"text":"plan","thought":true},{"text":"Hello"}]}}]});
        handle_chunk(&chunk, &mut got, &mut |e| events.push(e)).unwrap();
        assert!(got);
        assert_eq!(events[1], StreamEvent::Delta { text: "Hello".into() });

        let mut got = false;
        let blocked = json!({"candidates":[{"finishReason":"SAFETY"}]});
        assert!(handle_chunk(&blocked, &mut got, &mut |_| {}).is_err());
        let prompt_blocked = json!({"promptFeedback":{"blockReason":"OTHER"}});
        assert!(handle_chunk(&prompt_blocked, &mut got, &mut |_| {}).is_err());
    }
}
