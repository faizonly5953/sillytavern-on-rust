//! Chat-completion prompt builder with context budget.
//!
//! Order (SillyTavern's default prompt manager): main prompt, persona, description,
//! personality, scenario, example chats, "[Start a new Chat]", history,
//! post-history instructions, then the continue/impersonate nudge.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::character::CharacterData;
use crate::macros::{MacroContext, substitute};

pub use crate::macros::substitute as substitute_macros;
use crate::settings::{Persona, Settings};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct PromptMessage {
    pub role: Role,
    pub content: String,
    pub tokens: u32,
}

#[derive(Serialize, Clone, Debug)]
pub struct BuiltPrompt {
    pub messages: Vec<PromptMessage>,
    /// Text the reply must start with (Claude prefill).
    pub prefill: Option<String>,
    pub total_tokens: u32,
    pub budget: u32,
    pub history_included: usize,
    pub history_total: usize,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum GenMode {
    #[default]
    Normal,
    Continue,
    Impersonate,
    /// Rewrite the user's draft (`input`) as their next message.
    Improve,
}

/// Same heuristic SillyTavern uses when no tokenizer is available.
pub fn estimate_tokens(text: &str) -> u32 {
    (text.chars().count() as f32 / 3.35).ceil() as u32 + 4
}

fn message(role: Role, content: String) -> PromptMessage {
    let tokens = estimate_tokens(&content);
    PromptMessage { role, content, tokens }
}

fn push_nonempty(target: &mut Vec<PromptMessage>, role: Role, content: String) {
    if !content.trim().is_empty() {
        target.push(message(role, content));
    }
}

/// Splits `mes_example` on `<START>` (any case) into dialogue blocks.
fn split_examples(raw: &str) -> Vec<String> {
    let lower = raw.to_ascii_lowercase();
    let mut blocks = Vec::new();
    let mut last = 0;
    for (index, _) in lower.match_indices("<start>") {
        blocks.push(&raw[last..index]);
        last = index + "<start>".len();
    }
    blocks.push(&raw[last..]);
    blocks.into_iter().map(str::trim).filter(|b| !b.is_empty()).map(String::from).collect()
}

fn history_message(value: &Value) -> Option<PromptMessage> {
    if value.get("is_system").and_then(Value::as_bool).unwrap_or(false) {
        return None; // hidden from the AI, like SillyTavern
    }
    let content = value.get("mes").and_then(Value::as_str)?.trim();
    if content.is_empty() {
        return None;
    }
    let is_user = value.get("is_user").and_then(Value::as_bool).unwrap_or(false);
    Some(message(if is_user { Role::User } else { Role::Assistant }, content.to_string()))
}

pub fn macro_context<'a>(card: &'a CharacterData, persona: &'a Persona) -> MacroContext<'a> {
    MacroContext {
        user: &persona.name,
        char: &card.name,
        description: &card.description,
        personality: &card.personality,
        scenario: &card.scenario,
        persona: &persona.description,
        mes_examples: &card.mes_example,
        system_prompt: &card.system_prompt,
    }
}

/// `input` is the user's draft, only used by `GenMode::Improve`.
pub fn build(card: &CharacterData, persona: &Persona, settings: &Settings, history: &[Value], mode: GenMode, input: &str) -> BuiltPrompt {
    let ctx = macro_context(card, persona);
    let sub = |text: &str| substitute(text, &ctx).trim().to_string();
    let prompts = &settings.prompts;

    let mut head = Vec::new();
    let main = if prompts.prefer_char_prompt && !card.system_prompt.trim().is_empty() {
        card.system_prompt.replace("{{original}}", &prompts.main_prompt)
    } else {
        prompts.main_prompt.clone()
    };
    push_nonempty(&mut head, Role::System, sub(&main));
    push_nonempty(&mut head, Role::System, sub(&persona.description));
    push_nonempty(&mut head, Role::System, sub(&card.description));
    if !card.personality.trim().is_empty() {
        head.push(message(Role::System, format!("{}'s personality: {}", card.name, sub(&card.personality))));
    }
    if !card.scenario.trim().is_empty() {
        head.push(message(Role::System, format!("Scenario: {}", sub(&card.scenario))));
    }
    for block in split_examples(&card.mes_example) {
        head.push(message(Role::System, format!("{}\n{}", sub(&prompts.new_example_chat), sub(&block))));
    }
    push_nonempty(&mut head, Role::System, sub(&prompts.new_chat_prompt));

    let mut tail = Vec::new();
    let post = if prompts.prefer_char_instructions && !card.post_history_instructions.trim().is_empty() {
        card.post_history_instructions.replace("{{original}}", &prompts.post_history)
    } else {
        prompts.post_history.clone()
    };
    push_nonempty(&mut tail, Role::System, sub(&post));
    match mode {
        GenMode::Continue => push_nonempty(&mut tail, Role::System, sub(&prompts.continue_nudge)),
        GenMode::Impersonate => push_nonempty(&mut tail, Role::System, sub(&prompts.impersonation_prompt)),
        GenMode::Improve => tail.push(message(Role::System, format!("{}\n\nDraft:\n{}", sub(&prompts.improve_prompt), sub(input)))),
        GenMode::Normal => {}
    }

    let generation = &settings.generation;
    let budget = generation.context_size.saturating_sub(generation.max_tokens);
    let fixed: u32 = head.iter().chain(tail.iter()).map(|m| m.tokens).sum();
    let mut remaining = budget.saturating_sub(fixed);
    let candidates: Vec<PromptMessage> = history.iter().filter_map(history_message).collect();
    let mut picked = Vec::new();
    // Newest first; the latest message is always kept even if it overflows.
    for candidate in candidates.iter().rev() {
        if candidate.tokens > remaining && !picked.is_empty() {
            break;
        }
        remaining = remaining.saturating_sub(candidate.tokens);
        picked.push(candidate.clone());
    }
    picked.reverse();

    let history_included = picked.len();
    let mut messages = head;
    messages.extend(picked);
    messages.extend(tail);
    let prefill = (mode == GenMode::Normal).then(|| sub(&prompts.assistant_prefill)).filter(|p| !p.is_empty());
    BuiltPrompt {
        total_tokens: messages.iter().map(|m| m.tokens).sum(),
        messages,
        prefill,
        budget,
        history_included,
        history_total: candidates.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn card() -> CharacterData {
        CharacterData {
            name: "Ann".into(),
            description: "{{char}} likes {{user}}.".into(),
            personality: "calm".into(),
            mes_example: "<START>\n{{user}}: hi\n{{char}}: hello\n<start>\n{{char}}: bye".into(),
            post_history_instructions: "Stay in character. {{original}}".into(),
            ..Default::default()
        }
    }

    fn persona() -> Persona {
        Persona { id: "p".into(), name: "Faiz".into(), ..Default::default() }
    }

    #[test]
    fn order_and_macros() {
        let mut settings = Settings::default();
        settings.prompts.post_history = "Be vivid.".into();
        let history = vec![
            json!({"is_user": false, "mes": "Welcome"}),
            json!({"is_user": true, "mes": "Hi"}),
            json!({"is_user": true, "is_system": true, "mes": "hidden"}),
        ];
        let built = build(&card(), &persona(), &settings, &history, GenMode::Normal, "");
        let roles: Vec<Role> = built.messages.iter().map(|m| m.role).collect();
        use Role::*;
        assert_eq!(roles, vec![System, System, System, System, System, System, Assistant, User, System]);
        assert_eq!(built.messages[1].content, "Ann likes Faiz.");
        assert_eq!(built.messages[2].content, "Ann's personality: calm");
        assert!(built.messages[3].content.ends_with("Faiz: hi\nAnn: hello"));
        assert_eq!(built.messages.last().unwrap().content, "Stay in character. Be vivid.");
        assert_eq!((built.history_included, built.history_total), (2, 2));
    }

    #[test]
    fn trims_old_history_to_budget() {
        let mut settings = Settings::default();
        settings.generation.context_size = 300;
        settings.generation.max_tokens = 100;
        let history: Vec<Value> = (0..50).map(|i| json!({"is_user": i % 2 == 0, "mes": format!("message number {i} {}", "x".repeat(60))})).collect();
        let built = build(&CharacterData { name: "B".into(), ..Default::default() }, &persona(), &settings, &history, GenMode::Normal, "");
        assert!(built.history_included > 0 && built.history_included < 50);
        assert!(built.total_tokens <= built.budget);
        assert!(built.messages.iter().any(|m| m.content.starts_with("message number 49")));
    }

    #[test]
    fn modes_add_nudges() {
        let settings = Settings::default();
        let c = build(&card(), &persona(), &settings, &[], GenMode::Continue, "");
        assert!(c.messages.last().unwrap().content.contains("Continue your last message"));
        let i = build(&card(), &persona(), &settings, &[], GenMode::Impersonate, "");
        assert!(i.messages.last().unwrap().content.contains("point of view of Faiz"));
        // Improve: the draft (macros filled) closes the prompt, after the instruction.
        let history = [json!({"is_user": false, "mes": "Welcome"})];
        let m = build(&card(), &persona(), &settings, &history, GenMode::Improve, "{{user}} opens the door slowly");
        let last = &m.messages.last().unwrap().content;
        assert!(last.contains("Rewrite the draft below as Faiz's next message"));
        assert!(last.ends_with("Draft:\nFaiz opens the door slowly"));
        assert!(m.prefill.is_none());
    }
}
