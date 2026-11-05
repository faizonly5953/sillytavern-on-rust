//! Import character cards from a URL, mirroring SillyTavern's "Import from URL":
//! Chub / CharacterHub, Pygmalion, JanitorAI (via JannyAI), AICharacterCards,
//! RisuRealm, or any direct link to a PNG / JSON card.

use std::time::Duration;

use reqwest::Url;
use serde_json::{Value, json};

use crate::character::{self, CharacterData};
use crate::error::{AppError, AppResult, invalid};

/// A downloaded card: either a ready file (PNG/JSON) or card data plus artwork.
pub enum Downloaded {
    File { bytes: Vec<u8>, name: String },
    Card { card: Box<CharacterData>, image: Option<Vec<u8>> },
}

#[derive(Debug, PartialEq)]
enum Source {
    Chub { creator: String, slug: String },
    Pygmalion(String),
    Janny(String),
    Aicc(String),
    Risu(String),
    Direct,
}

/// First `8-4-4-4-12` hex UUID anywhere in the text.
fn find_uuid(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let pattern = [8, 4, 4, 4, 12];
    (0..bytes.len()).find_map(|start| {
        let mut pos = start;
        for (group, len) in pattern.iter().enumerate() {
            if group > 0 {
                if bytes.get(pos) != Some(&b'-') {
                    return None;
                }
                pos += 1;
            }
            let end = pos + len;
            if end > bytes.len() || !bytes[pos..end].iter().all(u8::is_ascii_hexdigit) {
                return None;
            }
            pos = end;
        }
        Some(text[start..pos].to_string())
    })
}

fn classify(url: &Url) -> AppResult<Source> {
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    let segments: Vec<&str> = url.path_segments().map(|s| s.filter(|p| !p.is_empty()).collect()).unwrap_or_default();
    let missing = || invalid("This link does not point to a character");

    if host.ends_with("chub.ai") || host.ends_with("characterhub.org") {
        let rest: &[&str] = match segments.first() {
            Some(first) if first.eq_ignore_ascii_case("characters") => &segments[1..],
            Some(first) if first.eq_ignore_ascii_case("lorebooks") => {
                return Err(invalid("Lorebooks are not supported yet"));
            }
            _ => &segments[..],
        };
        return match rest {
            [creator, slug, ..] => Ok(Source::Chub { creator: creator.to_string(), slug: slug.to_string() }),
            _ => Err(missing()),
        };
    }
    if host.ends_with("pygmalion.chat") {
        return find_uuid(url.as_str()).map(Source::Pygmalion).ok_or_else(missing);
    }
    if host.contains("janitorai") || host.contains("jannyai") {
        return find_uuid(url.as_str()).map(Source::Janny).ok_or_else(missing);
    }
    if host.ends_with("aicharactercards.com") {
        return match segments.as_slice() {
            [.., author, name] => Ok(Source::Aicc(format!("{author}/{name}"))),
            _ => Err(missing()),
        };
    }
    if host == "realm.risuai.net" {
        return match segments.as_slice() {
            ["character", id, ..] if id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') => {
                Ok(Source::Risu(id.to_string()))
            }
            _ => Err(missing()),
        };
    }
    Ok(Source::Direct)
}

async fn fetch(client: &reqwest::Client, req: reqwest::RequestBuilder) -> AppResult<reqwest::Response> {
    let _ = client;
    let resp = req.timeout(Duration::from_secs(60)).send().await?;
    crate::providers::check_status(resp).await
}

async fn get_bytes(client: &reqwest::Client, url: &str) -> AppResult<Vec<u8>> {
    Ok(fetch(client, client.get(url)).await?.bytes().await?.to_vec())
}

fn file_name_from(url: &Url, fallback: &str) -> String {
    url.path_segments()
        .and_then(|mut s| s.next_back().map(String::from))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

pub async fn download(client: &reqwest::Client, raw: &str) -> AppResult<Downloaded> {
    let url = Url::parse(raw.trim()).map_err(|_| invalid("Enter a full link starting with https://"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(invalid("Only http and https links are supported"));
    }
    match classify(&url)? {
        Source::Chub { creator, slug } => {
            let api = format!("https://api.chub.ai/api/characters/{creator}/{slug}?full=true");
            let meta: Value = fetch(client, client.get(api).header("Accept", "application/json")).await?.json().await?;
            let node = meta.get("node").ok_or_else(|| AppError::Api("Chub returned no character".into()))?;
            let def = node.get("definition").cloned().unwrap_or_default();
            let field = |k: &str| def.get(k).cloned().unwrap_or(Value::Null);
            // Same field mapping SillyTavern uses for Chub cards.
            let card = json!({
                "spec": "chara_card_v2",
                "spec_version": "2.0",
                "data": {
                    "name": field("name"),
                    "description": field("personality"),
                    "personality": field("tavern_personality"),
                    "scenario": field("scenario"),
                    "first_mes": field("first_message"),
                    "mes_example": field("example_dialogs"),
                    "creator_notes": field("description"),
                    "system_prompt": field("system_prompt"),
                    "post_history_instructions": field("post_history_instructions"),
                    "alternate_greetings": field("alternate_greetings"),
                    "tags": node.get("topics").cloned().unwrap_or(json!([])),
                    "creator": creator,
                    "character_version": "",
                    "character_book": field("embedded_lorebook"),
                    "extensions": def.get("extensions").cloned().unwrap_or(json!({})),
                }
            });
            let card = character::parse_card_json(&card.to_string())?;
            let image = match node.get("max_res_url").and_then(Value::as_str) {
                Some(img) => get_bytes(client, img).await.ok(),
                None => None,
            };
            Ok(Downloaded::Card { card: Box::new(card), image })
        }
        Source::Pygmalion(id) => {
            let api = format!("https://server.pygmalion.chat/api/export/character/{id}/v2");
            let body: Value = fetch(client, client.get(api)).await?.json().await?;
            let data = body.get("character").filter(|c| c.is_object()).ok_or_else(|| AppError::Api("Pygmalion returned no character".into()))?;
            let card = character::parse_card_json(&data.to_string())?;
            let image = match data.pointer("/data/avatar").and_then(Value::as_str) {
                Some(img) => get_bytes(client, img).await.ok(),
                None => None,
            };
            Ok(Downloaded::Card { card: Box::new(card), image })
        }
        Source::Janny(id) => {
            let resp: Value = fetch(client, client.post("https://api.jannyai.com/api/v1/download").json(&json!({ "characterId": id })))
                .await?
                .json()
                .await?;
            let link = resp
                .get("downloadUrl")
                .and_then(Value::as_str)
                .filter(|_| resp.get("status").and_then(Value::as_str) == Some("ok"))
                .ok_or_else(|| AppError::Api("JannyAI could not provide this character".into()))?;
            Ok(Downloaded::File { bytes: get_bytes(client, link).await?, name: format!("{id}.png") })
        }
        Source::Aicc(path) => {
            let api = format!("https://aicharactercards.com/wp-json/pngapi/v1/image/{path}");
            Ok(Downloaded::File { bytes: get_bytes(client, &api).await?, name: "card.png".into() })
        }
        Source::Risu(id) => {
            let api = format!("https://realm.risuai.net/api/v1/download/png-v3/{id}?non_commercial=true");
            Ok(Downloaded::File { bytes: get_bytes(client, &api).await?, name: format!("{id}.png") })
        }
        Source::Direct => {
            let bytes = get_bytes(client, url.as_str()).await?;
            Ok(Downloaded::File { bytes, name: file_name_from(&url, "card.png") })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(url: &str) -> Source {
        classify(&Url::parse(url).unwrap()).unwrap()
    }

    #[test]
    fn recognises_sites() {
        assert_eq!(
            source("https://chub.ai/characters/someone/cool-bot-1234"),
            Source::Chub { creator: "someone".into(), slug: "cool-bot-1234".into() }
        );
        assert_eq!(
            source("https://www.characterhub.org/someone/cool-bot"),
            Source::Chub { creator: "someone".into(), slug: "cool-bot".into() }
        );
        assert_eq!(
            source("https://janitorai.com/characters/0a1b2c3d-4e5f-6789-abcd-ef0123456789_character-name"),
            Source::Janny("0a1b2c3d-4e5f-6789-abcd-ef0123456789".into())
        );
        assert_eq!(
            source("https://pygmalion.chat/character/0a1b2c3d-4e5f-6789-abcd-ef0123456789"),
            Source::Pygmalion("0a1b2c3d-4e5f-6789-abcd-ef0123456789".into())
        );
        assert_eq!(source("https://aicharactercards.com/character-cards/fantasy/bob/the-knight/"), Source::Aicc("bob/the-knight".into()));
        assert_eq!(source("https://realm.risuai.net/character/7adb0ed8d81855c8"), Source::Risu("7adb0ed8d81855c8".into()));
        assert_eq!(source("https://files.example.com/cards/seraphina.png"), Source::Direct);
        assert!(classify(&Url::parse("https://chub.ai/lorebooks/a/b").unwrap()).is_err());
        assert!(classify(&Url::parse("https://janitorai.com/characters/no-id").unwrap()).is_err());
    }

    #[test]
    fn finds_uuid_inside_text() {
        assert_eq!(find_uuid("x/0A1B2C3D-4E5F-6789-ABCD-EF0123456789_y"), Some("0A1B2C3D-4E5F-6789-ABCD-EF0123456789".into()));
        assert_eq!(find_uuid("0a1b2c3d-4e5f-6789-abcd"), None);
    }
}
