//! "Create with AI": turns a plain description into a character card by asking the
//! configured LLM for one JSON object, then keeps only the card fields the app knows.

use serde_json::{Map, Value};

use crate::error::{AppResult, invalid};
use crate::prompt::{BuiltPrompt, PromptMessage, Role};

/// A whole card is long; the chat's usual response length (a few hundred tokens) would cut it off.
pub const MIN_TOKENS: u32 = 3000;

const STRING_FIELDS: [&str; 7] =
    ["name", "description", "personality", "scenario", "first_mes", "mes_example", "creator_notes"];
const LIST_FIELDS: [&str; 2] = ["tags", "alternate_greetings"];

const SYSTEM_PROMPT: &str = r#"You design roleplay character cards for a chat app. The user describes the character they want. Reply with ONE JSON object and nothing else (no markdown fences, no commentary) that has exactly these keys:
- "name": the character's name (short).
- "description": who the character is: appearance, background, abilities, relationships. Several detailed paragraphs.
- "personality": a compact summary of temperament, speech style and habits.
- "scenario": the setting and situation in which the chat with {{user}} begins.
- "first_mes": the character's opening message to {{user}}, in the character's voice. Actions in *asterisks*, speech as plain text. 2-4 paragraphs.
- "mes_example": two or three short example exchanges that show the voice. Start each exchange with a line "<START>", then lines like "{{user}}: ..." and "{{char}}: ...".
- "creator_notes": one or two sentences for the person reading the card: what the character is about and how to use it.
- "tags": an array of 3-8 short lowercase tags.
- "alternate_greetings": an array of 1-2 other opening messages (may be empty).
Rules: write in the language of the user's description unless they ask otherwise. Use {{char}} for the character's name and {{user}} for the person chatting; never invent a name for the user. Never write {{user}}'s actions, words or thoughts in first_mes or alternate_greetings. Stick to what the user asked for and only invent what is needed to make the character feel complete. Escape everything inside the JSON strings properly (use \n for line breaks)."#;

pub fn prompt(description: &str) -> BuiltPrompt {
    let message = |role, content: String| PromptMessage { role, content, tokens: 0 };
    BuiltPrompt {
        messages: vec![
            message(Role::System, SYSTEM_PROMPT.to_string()),
            message(Role::User, format!("Character I want:\n{}", description.trim())),
        ],
        prefill: None,
        total_tokens: 0,
        budget: 0,
        history_included: 0,
        history_total: 0,
    }
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

/// Pulls the card out of the model's reply. Tolerates code fences, text around the JSON and the
/// `{"data": {...}}` wrapper of V2 cards; unknown keys and wrongly typed values are dropped.
pub fn parse_card(reply: &str) -> AppResult<Value> {
    let (start, end) = reply.find('{').zip(reply.rfind('}')).filter(|(s, e)| s < e).ok_or_else(|| {
        invalid("The model did not return a character card. Try again, or describe the character in more detail.")
    })?;
    let parsed: Value = serde_json::from_str(&reply[start..=end])
        .map_err(|_| invalid("The model's reply was not a valid character card (it may have been cut off). Try again."))?;
    let card = parsed.get("data").filter(|d| d.get("name").is_some()).unwrap_or(&parsed);

    let mut out = Map::new();
    for key in STRING_FIELDS {
        if let Some(text) = card.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()) {
            out.insert(key.to_string(), Value::String(text.to_string()));
        }
    }
    for key in LIST_FIELDS {
        if let Some(list) = card.get(key).and_then(clean_list) {
            out.insert(key.to_string(), list);
        }
    }
    if !out.contains_key("name") {
        return Err(invalid("The model did not give the character a name. Try again."));
    }
    Ok(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_json_inside_fences_and_prose() {
        let reply = "Sure! Here it is:\n```json\n{\"name\": \"Mira\", \"description\": \" A vampire hunter. \", \"tags\": [\"gothic\", \" \", \"hunter\"]}\n```\nEnjoy.";
        let card = parse_card(reply).unwrap();
        assert_eq!(card, json!({ "name": "Mira", "description": "A vampire hunter.", "tags": ["gothic", "hunter"] }));
    }

    #[test]
    fn unwraps_v2_data_and_drops_unknown_or_mistyped_fields() {
        let reply = r#"{"spec":"chara_card_v2","data":{"name":"Rook","personality":42,"extensions":{"x":1},"alternate_greetings":["Hi.",7],"first_mes":"*waves*"}}"#;
        let card = parse_card(reply).unwrap();
        assert_eq!(card, json!({ "name": "Rook", "first_mes": "*waves*", "alternate_greetings": ["Hi."] }));
    }

    #[test]
    fn rejects_replies_without_a_usable_card() {
        assert!(parse_card("I cannot do that.").is_err());
        assert!(parse_card("{\"name\": \"Cut off, \"description\": ").is_err());
        assert!(parse_card("{\"description\": \"no name\"}").is_err());
        assert!(parse_card("{\"name\": \"   \"}").is_err());
    }

    #[test]
    fn prompt_has_system_then_user_with_the_description() {
        let p = prompt("  a sarcastic bard \n");
        assert_eq!(p.messages.len(), 2);
        assert_eq!((p.messages[0].role, p.messages[1].role), (Role::System, Role::User));
        assert!(p.messages[1].content.ends_with("a sarcastic bard"));
    }
}
