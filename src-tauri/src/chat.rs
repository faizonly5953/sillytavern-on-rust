//! Chats stored as SillyTavern JSONL (header line + one message per line).
//!
//! Header: `{"user_name","character_name","create_date":"2026-11-05@20h41m03s","chat_metadata":{}}`
//! Message: `{"name","is_user","is_system","send_date","mes","swipes":[..],"swipe_id","extra":{}}`
//! Messages are kept as raw JSON so fields we don't model are never lost.

use std::fs;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::error::{AppError, AppResult, invalid};
use crate::paths::{self, DataDirs};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChatFile {
    pub file: String,
    pub header: Value,
    pub messages: Vec<Value>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ChatSummary {
    pub file: String,
    pub message_count: usize,
    pub last_message: String,
    pub modified: u64,
}

pub fn humanized_now() -> String {
    chrono::Local::now().format("%Y-%m-%d@%Hh%Mm%Ss").to_string()
}

/// SillyTavern-style display date, e.g. "November 5, 2026 8:41pm".
pub fn send_date_now() -> String {
    chrono::Local::now().format("%B %-d, %Y %-I:%M%P").to_string()
}

pub fn new_header(user_name: &str, character_name: &str) -> Value {
    json!({
        "user_name": user_name,
        "character_name": character_name,
        "create_date": humanized_now(),
        "chat_metadata": {},
    })
}

/// The opening message, with alternate greetings available as swipes.
pub fn greeting_message(character_name: &str, greetings: Vec<String>) -> Option<Value> {
    let first = greetings.first()?.clone();
    Some(json!({
        "name": character_name,
        "is_user": false,
        "is_system": false,
        "send_date": send_date_now(),
        "mes": first,
        "swipes": greetings,
        "swipe_id": 0,
        "swipe_info": [],
        "extra": {},
    }))
}

/// Splits JSONL into (header, messages). Unparseable lines are skipped, not fatal.
pub fn parse_jsonl(raw: &str) -> (Option<Value>, Vec<Value>) {
    let mut values = raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(Value::is_object)
        .peekable();
    let header = match values.peek() {
        Some(first) if first.get("mes").is_none() => values.next(),
        _ => None,
    };
    (header, values.collect())
}

pub fn to_jsonl(header: &Value, messages: &[Value]) -> AppResult<String> {
    let mut out = serde_json::to_string(header)?;
    for message in messages {
        out.push('\n');
        out.push_str(&serde_json::to_string(message)?);
    }
    Ok(out)
}

pub fn list(dirs: &DataDirs, character_file: &str) -> AppResult<Vec<ChatSummary>> {
    let dir = dirs.chat_dir(character_file)?;
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let Some(file) = path.file_name().and_then(|n| n.to_str()).map(String::from) else { continue };
        let raw = fs::read_to_string(&path).unwrap_or_default();
        let (_, messages) = parse_jsonl(&raw);
        let last_message = messages
            .last()
            .and_then(|m| m.get("mes"))
            .and_then(Value::as_str)
            .map(|s| s.chars().take(140).collect())
            .unwrap_or_default();
        out.push(ChatSummary { file, message_count: messages.len(), last_message, modified: paths::modified_ms(&path) });
    }
    out.sort_by_key(|c| std::cmp::Reverse(c.modified));
    Ok(out)
}

pub fn load(dirs: &DataDirs, character_file: &str, chat_file: &str) -> AppResult<ChatFile> {
    let path = dirs.chat_file(character_file, chat_file)?;
    if !path.exists() {
        return Err(AppError::NotFound(format!("chat {chat_file}")));
    }
    let (header, messages) = parse_jsonl(&fs::read_to_string(path)?);
    Ok(ChatFile { file: chat_file.to_string(), header: header.unwrap_or_else(|| new_header("User", "")), messages })
}

pub fn create(dirs: &DataDirs, character_file: &str, header: Value, messages: Vec<Value>) -> AppResult<ChatFile> {
    let dir = dirs.chat_dir(character_file)?;
    fs::create_dir_all(&dir)?;
    let character_name = header.get("character_name").and_then(Value::as_str).unwrap_or("Chat");
    let stem = paths::sanitize_file_stem(&format!("{character_name} - {}", humanized_now()));
    let file = paths::unique_file_name(&dir, &stem, "jsonl");
    paths::write_atomic(&dir.join(&file), to_jsonl(&header, &messages)?.as_bytes())?;
    Ok(ChatFile { file, header, messages })
}

pub fn save(dirs: &DataDirs, character_file: &str, chat_file: &str, header: &Value, messages: &[Value]) -> AppResult<()> {
    let path = dirs.chat_file(character_file, chat_file)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    paths::write_atomic(&path, to_jsonl(header, messages)?.as_bytes())
}

pub fn delete(dirs: &DataDirs, character_file: &str, chat_file: &str) -> AppResult<()> {
    fs::remove_file(dirs.chat_file(character_file, chat_file)?)?;
    Ok(())
}

pub fn rename(dirs: &DataDirs, character_file: &str, chat_file: &str, new_name: &str) -> AppResult<String> {
    let stem = paths::sanitize_file_stem(new_name.trim().trim_end_matches(".jsonl"));
    let target = format!("{stem}.jsonl");
    let from = dirs.chat_file(character_file, chat_file)?;
    let to = dirs.chat_file(character_file, &target)?;
    if target == chat_file {
        return Ok(target);
    }
    if to.exists() {
        return Err(invalid(format!("A chat named \"{stem}\" already exists")));
    }
    fs::rename(from, to)?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_with_and_without_header() {
        let (header, messages) = parse_jsonl("{\"user_name\":\"U\"}\n{\"mes\":\"a\"}\n\nnot json\n{\"mes\":\"b\"}\n");
        assert!(header.is_some());
        assert_eq!(messages.len(), 2);
        let (header, messages) = parse_jsonl("{\"mes\":\"only\"}");
        assert!(header.is_none());
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn create_save_list_rename_delete() {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = DataDirs::new(tmp.path().to_path_buf()).unwrap();
        let greeting = greeting_message("Ann", vec!["hi".into(), "hey".into()]).unwrap();
        let chat = create(&dirs, "Ann.png", new_header("U", "Ann"), vec![greeting]).unwrap();
        assert!(chat.file.starts_with("Ann - ") && chat.file.ends_with(".jsonl"));

        let mut messages = chat.messages.clone();
        messages.push(json!({"name":"U","is_user":true,"mes":"hello","unknown_field":1}));
        save(&dirs, "Ann.png", &chat.file, &chat.header, &messages).unwrap();
        let loaded = load(&dirs, "Ann.png", &chat.file).unwrap();
        assert_eq!(loaded.messages, messages);

        let listed = list(&dirs, "Ann.png").unwrap();
        assert_eq!((listed[0].message_count, listed[0].last_message.as_str()), (2, "hello"));

        let renamed = rename(&dirs, "Ann.png", &chat.file, "Beach day").unwrap();
        assert_eq!(renamed, "Beach day.jsonl");
        delete(&dirs, "Ann.png", &renamed).unwrap();
        assert!(list(&dirs, "Ann.png").unwrap().is_empty());
        assert!(load(&dirs, "Ann.png", "../x.jsonl").is_err());
    }
}
