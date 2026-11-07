//! App settings, personas and API key storage (settings.json / secrets.json).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::paths;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    #[default]
    Openai,
    Claude,
    Gemini,
}

impl Provider {
    pub const ALL: [Provider; 3] = [Provider::Openai, Provider::Claude, Provider::Gemini];

    pub fn as_str(self) -> &'static str {
        match self {
            Provider::Openai => "openai",
            Provider::Claude => "claude",
            Provider::Gemini => "gemini",
        }
    }

    /// Same key names as SillyTavern's secrets.json.
    pub fn secret_key(self) -> &'static str {
        match self {
            Provider::Openai => "api_key_openai",
            Provider::Claude => "api_key_claude",
            Provider::Gemini => "api_key_makersuite",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct ProviderConn {
    pub base_url: String,
    pub model: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Connections {
    pub openai: ProviderConn,
    pub claude: ProviderConn,
    pub gemini: ProviderConn,
}

impl Default for Connections {
    fn default() -> Self {
        let conn = |base_url: &str, model: &str| ProviderConn { base_url: base_url.into(), model: model.into() };
        Self {
            openai: conn("https://api.openai.com/v1", ""),
            claude: conn("https://api.anthropic.com/v1", "claude-sonnet-5-5"),
            gemini: conn("https://generativelanguage.googleapis.com/v1beta", ""),
        }
    }
}

impl Connections {
    pub fn get(&self, provider: Provider) -> &ProviderConn {
        match provider {
            Provider::Openai => &self.openai,
            Provider::Claude => &self.claude,
            Provider::Gemini => &self.gemini,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct GenerationParams {
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: u32,
    /// Response length.
    pub max_tokens: u32,
    /// Total context window the prompt must fit in (prompt + response).
    pub context_size: u32,
    pub frequency_penalty: f32,
    pub presence_penalty: f32,
    pub stream: bool,
}

impl Default for GenerationParams {
    fn default() -> Self {
        Self {
            temperature: 1.0,
            top_p: 1.0,
            top_k: 0,
            max_tokens: 400,
            context_size: 8192,
            frequency_penalty: 0.0,
            presence_penalty: 0.0,
            stream: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct PromptSettings {
    pub main_prompt: String,
    pub post_history: String,
    pub prefer_char_prompt: bool,
    pub prefer_char_instructions: bool,
    pub impersonation_prompt: String,
    pub continue_nudge: String,
    /// Improve: rewrites the user's draft (appended after this prompt) as {{user}}'s message.
    pub improve_prompt: String,
    pub new_chat_prompt: String,
    pub new_example_chat: String,
    /// Claude only: text the reply is forced to start with.
    pub assistant_prefill: String,
    /// Sloth Mode: explains one of {{char}}'s messages (appended after this prompt).
    pub sloth_prompt: String,
    /// Language of Sloth Mode explanations; empty follows the language {{user}} writes in.
    pub sloth_language: String,
}

impl Default for PromptSettings {
    fn default() -> Self {
        Self {
            main_prompt: "Write {{char}}'s next reply in a fictional chat between {{char}} and {{user}}.".into(),
            post_history: String::new(),
            prefer_char_prompt: true,
            prefer_char_instructions: true,
            impersonation_prompt: "[Write your next reply from the point of view of {{user}}, using the chat history so far as a guideline for the writing style of {{user}}. Don't write as {{char}} or system. Don't describe actions of {{char}}.]".into(),
            continue_nudge: "[Continue your last message without repeating its original content.]".into(),
            improve_prompt: "[Rewrite the draft below as {{user}}'s next message in this roleplay. Keep every event, action, line of dialogue and intention it describes, in the same order, and follow it exactly. Do not add new decisions or actions for {{user}}, and never write {{char}}'s reaction or reply. Improve the prose, dialogue and sensory detail to match the story's style and point of view. Reply with the rewritten message only.]".into(),
            new_chat_prompt: "[Start a new Chat]".into(),
            new_example_chat: "[Example Chat]".into(),
            assistant_prefill: String::new(),
            sloth_prompt: "[Pause the roleplay. Explain the message from {{char}} below to {{user}} in plain, simple words, as a friend would. Cover: what {{char}} actually means, including anything implied or left unsaid; what {{char}} is feeling and why; what {{char}} is doing (actions and body language); and how it connects to what happened before in the story. If {{char}} asks or expects something from {{user}}, say what. Keep it short and easy to read, use a few short paragraphs or bullet points, and do not continue the story or write as {{char}}.]".into(),
            sloth_language: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Persona {
    pub id: String,
    pub name: String,
    /// Label shown only in the persona picker, to tell same-named personas apart.
    pub alias: String,
    pub description: String,
    /// File name inside `User Avatars/`.
    pub avatar: Option<String>,
}

fn default_persona() -> Persona {
    Persona { id: "default".into(), name: "User".into(), ..Default::default() }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct UiSettings {
    pub theme: String,
    pub font_scale: f32,
    pub last_character: Option<String>,
    pub last_chat: Option<String>,
    pub show_reasoning: bool,
    pub enter_to_send: bool,
    pub sloth_mode: bool,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            font_scale: 1.0,
            last_character: None,
            last_chat: None,
            show_reasoning: true,
            enter_to_send: true,
            sloth_mode: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    pub provider: Provider,
    pub connections: Connections,
    pub generation: GenerationParams,
    pub prompts: PromptSettings,
    pub personas: Vec<Persona>,
    pub active_persona: String,
    pub ui: UiSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            provider: Provider::default(),
            connections: Connections::default(),
            generation: GenerationParams::default(),
            prompts: PromptSettings::default(),
            personas: vec![default_persona()],
            active_persona: "default".into(),
            ui: UiSettings::default(),
        }
    }
}

impl Settings {
    pub fn persona(&self) -> Persona {
        self.personas
            .iter()
            .find(|p| p.id == self.active_persona)
            .or_else(|| self.personas.first())
            .cloned()
            .unwrap_or_else(default_persona)
    }
}

pub fn load_settings(path: &Path) -> Settings {
    match fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|e| {
            eprintln!("settings.json is invalid ({e}); a backup was kept and defaults are used");
            let _ = fs::copy(path, path.with_extension("json.bak"));
            Settings::default()
        }),
        Err(_) => Settings::default(),
    }
}

pub fn save_settings(path: &Path, settings: &Settings) -> AppResult<()> {
    paths::write_atomic(path, &serde_json::to_vec_pretty(settings)?)
}

pub type Secrets = BTreeMap<String, String>;

pub fn load_secrets(path: &Path) -> Secrets {
    fs::read_to_string(path).ok().and_then(|raw| serde_json::from_str(&raw).ok()).unwrap_or_default()
}

pub fn save_secrets(path: &Path, secrets: &Secrets) -> AppResult<()> {
    paths::write_atomic(path, &serde_json::to_vec_pretty(secrets)?)
}

/// Shows enough of a key to recognise it without exposing it.
pub fn mask_secret(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= 10 {
        return "\u{2022}".repeat(6);
    }
    let head: String = chars[..4].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}\u{2026}{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_json_fills_defaults() {
        let s: Settings = serde_json::from_str(r#"{"provider":"gemini","generation":{"temperature":0.7}}"#).unwrap();
        assert_eq!(s.provider, Provider::Gemini);
        assert_eq!(s.generation.temperature, 0.7);
        assert_eq!(s.generation.max_tokens, 400);
        assert_eq!(s.persona().name, "User");
    }

    #[test]
    fn invalid_file_is_backed_up() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        fs::write(&path, "{broken").unwrap();
        let s = load_settings(&path);
        assert_eq!(s.active_persona, "default");
        assert!(tmp.path().join("settings.json.bak").exists());
    }

    #[test]
    fn masks_keys() {
        assert_eq!(mask_secret("sk-1234567890abcd"), "sk-1\u{2026}abcd");
        assert_eq!(mask_secret("short"), "\u{2022}".repeat(6));
    }
}
