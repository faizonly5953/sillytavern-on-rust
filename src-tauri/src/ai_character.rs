//! "Create with AI": turns a plain description into a character card. The model writes plain
//! text sections (`=== NAME ===` ...) instead of JSON: long prose full of quoted dialogue and
//! line breaks is exactly what breaks JSON escaping, and a reply that is cut off still keeps
//! its finished sections. JSON replies are still understood, for models that insist on them.

use serde_json::{Map, Value};

use crate::error::{AppResult, invalid};
use crate::prompt::{BuiltPrompt, PromptMessage, Role};

/// A whole card is long; the chat's usual response length (a few hundred tokens) would cut it off.
pub const MIN_TOKENS: u32 = 4096;

const JSON_STRING_FIELDS: [&str; 7] =
    ["name", "description", "personality", "scenario", "first_mes", "mes_example", "creator_notes"];
const JSON_LIST_FIELDS: [&str; 2] = ["tags", "alternate_greetings"];

const SYSTEM_PROMPT: &str = r#"You write roleplay character cards for a chat app. The user describes the character they want. Write the card as plain text sections. Every section starts with its marker on a line of its own, in this order:

=== NAME ===
The character's name (short).
=== DESCRIPTION ===
Who the character is: appearance, background, abilities, relationships. Several detailed paragraphs.
=== PERSONALITY ===
A compact summary of temperament, speech style and habits.
=== SCENARIO ===
The setting and situation in which the chat with {{user}} begins.
=== FIRST_MESSAGE ===
The character's opening message to {{user}}, in the character's voice. Actions in *asterisks*, speech as plain text. 2-4 paragraphs.
=== EXAMPLE_DIALOGUE ===
Two or three short exchanges that show the voice. Start each exchange with a line <START>, then lines like {{user}}: ... and {{char}}: ...
=== CREATOR_NOTES ===
One or two sentences for the person reading the card: what the character is about and how to use it.
=== TAGS ===
3-8 short lowercase tags, separated by commas.
=== ALTERNATE_GREETING ===
Another opening message. Optional; repeat this marker for a second one.

Rules: output only these sections, starting with the NAME marker. No JSON, no code fences, no introduction and no closing remarks. Do not ask questions: if the request is vague, invent what is missing. Write in the language of the user's description unless they ask otherwise. Use {{char}} for the character's name and {{user}} for the person chatting; never invent a name for the user. Never write {{user}}'s actions, words or thoughts in FIRST_MESSAGE or ALTERNATE_GREETING. Stick to what the user asked for and invent only what is needed to make the character feel complete."#;

const RETRY_NUDGE: &str = "That was not in the required format. Write the card again now: start with === NAME ===, use only the === FIELD === sections from the instructions, and add no commentary.";

fn message(role: Role, content: impl Into<String>) -> PromptMessage {
    PromptMessage { role, content: content.into(), tokens: 0 }
}

pub fn prompt(description: &str) -> BuiltPrompt {
    BuiltPrompt {
        messages: vec![
            message(Role::System, SYSTEM_PROMPT),
            message(
                Role::User,
                format!(
                    "Character I want:\n{}\n\nWrite the card now. Start with === NAME === and use only the sections above.",
                    description.trim()
                ),
            ),
        ],
        prefill: None,
        total_tokens: 0,
        budget: 0,
        history_included: 0,
        history_total: 0,
    }
}

/// Prepares a second attempt after a reply that was not a card: the model sees what it wrote
/// and is told the format again. An empty reply is simply asked again as is.
pub fn retry_prompt(prompt: &mut BuiltPrompt, reply: &str) {
    if reply.trim().is_empty() {
        return;
    }
    prompt.messages.push(message(Role::Assistant, reply.chars().take(4000).collect::<String>()));
    prompt.messages.push(message(Role::User, RETRY_NUDGE));
}

/// "=== NAME ===", "### Name", "**DESCRIPTION**", "[First_Message]:" ... a line that is only a
/// known field name, whatever decoration the model put around it.
fn marker(line: &str) -> Option<&'static str> {
    if line.len() > 60 {
        return None;
    }
    let key = line
        .trim_matches(|c: char| c.is_whitespace() || "=#*[]:-_".contains(c))
        .to_lowercase()
        .replace([' ', '-'], "_");
    Some(match key.as_str() {
        "name" => "name",
        "description" => "description",
        "personality" => "personality",
        "scenario" => "scenario",
        "first_message" | "first_mes" => "first_mes",
        "example_dialogue" | "example_dialog" | "example_messages" | "mes_example" => "mes_example",
        "creator_notes" | "notes" => "creator_notes",
        "tags" => "tags",
        "alternate_greeting" | "alternate_greetings" => "alternate_greetings",
        _ => return None,
    })
}

/// Drops one code fence wrapped around the whole reply.
fn strip_outer_fence(reply: &str) -> &str {
    let text = reply.trim();
    let text = if text.starts_with("```") { text.split_once('\n').map_or("", |(_, rest)| rest) } else { text };
    text.trim_end().strip_suffix("```").unwrap_or(text)
}

/// Reads the section format. Text before the first marker (an introduction) is ignored and the
/// last section may be unfinished. Needs at least a name.
fn parse_sections(reply: &str) -> Option<Value> {
    let mut sections: Vec<(&'static str, String)> = Vec::new();
    for line in strip_outer_fence(reply).lines() {
        if let Some(key) = marker(line) {
            sections.push((key, String::new()));
        } else if let Some((_, text)) = sections.last_mut() {
            text.push_str(line);
            text.push('\n');
        }
    }
    let mut out = Map::new();
    for (key, text) in sections {
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        match key {
            "name" => {
                let first = text.lines().next().unwrap_or_default();
                let name = first.trim_matches(|c: char| c.is_whitespace() || "*#\"'`".contains(c));
                if !name.is_empty() {
                    out.entry(key).or_insert_with(|| Value::String(name.to_string()));
                }
            }
            "tags" => {
                let tags: Vec<Value> = text
                    .split([',', '\n'])
                    .map(|t| t.trim_matches(|c: char| c.is_whitespace() || "#*-\u{2022}\"'".contains(c)).to_lowercase())
                    .filter(|t| !t.is_empty())
                    .take(12)
                    .map(Value::String)
                    .collect();
                out.entry(key).or_insert(Value::Array(tags));
            }
            "alternate_greetings" => {
                if let Value::Array(list) = out.entry(key).or_insert_with(|| Value::Array(Vec::new()))
                    && list.len() < 3
                {
                    list.push(Value::String(text.to_string()));
                }
            }
            _ => {
                out.entry(key).or_insert_with(|| Value::String(text.to_string()));
            }
        }
    }
    out.contains_key("name").then(|| Value::Object(out))
}

fn clean_list(value: &Value) -> Option<Value> {
    let items: Vec<Value> = value
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .take(12)
        .map(|s| Value::String(s.to_string()))
        .collect();
    Some(Value::Array(items))
}

/// Fallback for models that answer with JSON anyway. Tolerates code fences, text around the
/// JSON and the `{"data": {...}}` wrapper of V2 cards; unknown keys and mistyped values are dropped.
fn parse_json(reply: &str) -> Option<Value> {
    let (start, end) = reply.find('{').zip(reply.rfind('}')).filter(|(s, e)| s < e)?;
    let parsed: Value = serde_json::from_str(&reply[start..=end]).ok()?;
    let card = parsed.get("data").filter(|d| d.get("name").is_some()).unwrap_or(&parsed);

    let mut out = Map::new();
    for key in JSON_STRING_FIELDS {
        if let Some(text) = card.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()) {
            out.insert(key.to_string(), Value::String(text.to_string()));
        }
    }
    for key in JSON_LIST_FIELDS {
        if let Some(list) = card.get(key).and_then(clean_list) {
            out.insert(key.to_string(), list);
        }
    }
    out.contains_key("name").then(|| Value::Object(out))
}

/// What the model said, for the error message: tells the user whether it refused, asked a
/// question, or sent nothing at all (a reasoning model can spend the whole token limit thinking).
fn excerpt(reply: &str) -> String {
    let flat = reply.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.is_empty() {
        return " (the reply was empty; a reasoning model may have used up the token limit)".into();
    }
    let short: String = flat.chars().take(160).collect();
    let more = if flat.chars().count() > 160 { "\u{2026}" } else { "" };
    format!(" (it said: \u{201c}{short}{more}\u{201d})")
}

/// The card fields from the model's reply (sections first, then JSON).
pub fn parse_card(reply: &str) -> AppResult<Value> {
    parse_sections(reply).or_else(|| parse_json(reply)).ok_or_else(|| {
        invalid(format!(
            "The model did not write a character card{}. Try again, or describe the character in more detail.",
            excerpt(reply)
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_sections_with_quotes_and_line_breaks() {
        // The text that breaks JSON: dialogue quotes and blank lines inside long prose.
        let reply = "Sure! Here is the card:\n\n=== NAME ===\nMira \"Nightshade\" Vale\n=== DESCRIPTION ===\nA hunter.\n\nShe says \"Stay back,\" and means it.\n=== PERSONALITY ===\nDry.\n=== TAGS ===\ngothic, #Hunter, - cats\n=== ALTERNATE_GREETING ===\n*Hi.*\n=== ALTERNATE_GREETING ===\n*Hello.*\n";
        assert_eq!(
            parse_card(reply).unwrap(),
            json!({
                "name": "Mira \"Nightshade\" Vale",
                "description": "A hunter.\n\nShe says \"Stay back,\" and means it.",
                "personality": "Dry.",
                "tags": ["gothic", "hunter", "cats"],
                "alternate_greetings": ["*Hi.*", "*Hello.*"],
            })
        );
    }

    #[test]
    fn accepts_marker_variants_and_a_fenced_reply() {
        let reply = "```\n### Name\nRook\n**DESCRIPTION**\nTall.\n=== first message ===\n*waves*\n```";
        assert_eq!(parse_card(reply).unwrap(), json!({ "name": "Rook", "description": "Tall.", "first_mes": "*waves*" }));
    }

    #[test]
    fn keeps_finished_sections_of_a_cut_off_reply() {
        let card = parse_card("=== NAME ===\nMira\n=== DESCRIPTION ===\nShe was born in a town that").unwrap();
        assert_eq!(card, json!({ "name": "Mira", "description": "She was born in a town that" }));
    }

    #[test]
    fn falls_back_to_json_in_fences_and_prose() {
        let reply = "Sure! Here it is:\n```json\n{\"name\": \"Mira\", \"description\": \" A vampire hunter. \", \"tags\": [\"gothic\", \" \", \"hunter\"]}\n```\nEnjoy.";
        let card = parse_card(reply).unwrap();
        assert_eq!(card, json!({ "name": "Mira", "description": "A vampire hunter.", "tags": ["gothic", "hunter"] }));
    }

    #[test]
    fn json_unwraps_v2_data_and_drops_unknown_or_mistyped_fields() {
        let reply = r#"{"spec":"chara_card_v2","data":{"name":"Rook","personality":42,"extensions":{"x":1},"alternate_greetings":["Hi.",7],"first_mes":"*waves*"}}"#;
        let card = parse_card(reply).unwrap();
        assert_eq!(card, json!({ "name": "Rook", "first_mes": "*waves*", "alternate_greetings": ["Hi."] }));
    }

    #[test]
    fn rejects_replies_without_a_usable_card() {
        assert!(parse_card("{\"name\": \"Cut off, \"description\": ").is_err());
        assert!(parse_card("{\"description\": \"no name\"}").is_err());
        assert!(parse_card("{\"name\": \"   \"}").is_err());
        assert!(parse_card("=== DESCRIPTION ===\nno name section").is_err());
    }

    #[test]
    fn error_says_what_the_model_said() {
        let refusal = parse_card("I'm sorry, but I can't help with that request.").unwrap_err().to_string();
        assert!(refusal.contains("I'm sorry, but I can't help"), "{refusal}");
        assert!(parse_card("  \n ").unwrap_err().to_string().contains("empty"));
        let long = parse_card(&"word ".repeat(100)).unwrap_err().to_string();
        assert!(long.contains('\u{2026}'), "{long}");
    }

    #[test]
    fn prompt_has_system_then_user_with_the_description() {
        let p = prompt("  a sarcastic bard \n");
        assert_eq!(p.messages.len(), 2);
        assert_eq!((p.messages[0].role, p.messages[1].role), (Role::System, Role::User));
        assert!(p.messages[1].content.contains("a sarcastic bard"));
        assert!(p.messages[1].content.contains("=== NAME ==="));
    }

    #[test]
    fn retry_shows_the_model_its_reply_and_skips_empty_ones() {
        let mut p = prompt("a bard");
        retry_prompt(&mut p, "   ");
        assert_eq!(p.messages.len(), 2);
        retry_prompt(&mut p, "Could you tell me more?");
        let roles: Vec<Role> = p.messages.iter().map(|m| m.role).collect();
        assert_eq!(roles, [Role::System, Role::User, Role::Assistant, Role::User]);
        assert_eq!(p.messages[2].content, "Could you tell me more?");
    }
}
