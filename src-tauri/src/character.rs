//! Character cards: V1/V2/V3 parsing, PNG import/export, create/update/delete.
//!
//! Cards are stored like SillyTavern: `characters/<name>.png` with the card JSON
//! base64-encoded in tEXt chunks `chara` (V2) and `ccv3` (V3).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Cursor;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use image::imageops::FilterType;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::error::{AppError, AppResult, invalid};
use crate::paths::{self, DataDirs};
use crate::png_meta;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct CharacterData {
    pub name: String,
    pub description: String,
    pub personality: String,
    pub scenario: String,
    pub first_mes: String,
    pub mes_example: String,
    pub creator_notes: String,
    pub system_prompt: String,
    pub post_history_instructions: String,
    pub alternate_greetings: Vec<String>,
    pub tags: Vec<String>,
    pub creator: String,
    pub character_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character_book: Option<Value>,
    pub extensions: Map<String, Value>,
    /// Fields we don't model (V3 extras such as `nickname`, `assets`) survive round-trips.
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

const KNOWN_FIELDS: &[&str] = &[
    "name", "description", "personality", "scenario", "first_mes", "mes_example", "creator_notes",
    "system_prompt", "post_history_instructions", "alternate_greetings", "tags", "creator",
    "character_version", "character_book", "extensions",
];

fn text(obj: &Map<String, Value>, key: &str) -> String {
    match obj.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn string_list(obj: &Map<String, Value>, key: &str) -> Vec<String> {
    match obj.get(key) {
        Some(Value::Array(items)) => items.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
        Some(Value::String(s)) => s.split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect(),
        _ => Vec::new(),
    }
}

impl CharacterData {
    fn from_object(obj: &Map<String, Value>, keep_unknown: bool) -> Self {
        Self {
            name: text(obj, "name"),
            description: text(obj, "description"),
            personality: text(obj, "personality"),
            scenario: text(obj, "scenario"),
            first_mes: text(obj, "first_mes"),
            mes_example: text(obj, "mes_example"),
            creator_notes: text(obj, "creator_notes"),
            system_prompt: text(obj, "system_prompt"),
            post_history_instructions: text(obj, "post_history_instructions"),
            alternate_greetings: string_list(obj, "alternate_greetings"),
            tags: string_list(obj, "tags"),
            creator: text(obj, "creator"),
            character_version: text(obj, "character_version"),
            character_book: obj.get("character_book").filter(|v| v.is_object()).cloned(),
            extensions: obj.get("extensions").and_then(Value::as_object).cloned().unwrap_or_default(),
            other: if keep_unknown {
                obj.iter()
                    .filter(|(k, _)| !KNOWN_FIELDS.contains(&k.as_str()))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            } else {
                Map::new()
            },
        }
    }

    pub fn is_favorite(&self) -> bool {
        self.extensions.get("fav").and_then(Value::as_bool).unwrap_or(false)
    }

    /// First message plus alternate greetings, empty ones dropped.
    pub fn greetings(&self) -> Vec<String> {
        std::iter::once(&self.first_mes)
            .chain(self.alternate_greetings.iter())
            .filter(|g| !g.trim().is_empty())
            .cloned()
            .collect()
    }
}

/// Parses V1 (flat), Pygmalion, V2 and V3 JSON cards.
pub fn parse_card_json(raw: &str) -> AppResult<CharacterData> {
    let root: Value = serde_json::from_str(raw.trim_start_matches('\u{feff}'))?;
    let obj = root.as_object().ok_or_else(|| invalid("Character card is not a JSON object"))?;
    let mut card = match obj.get("data").and_then(Value::as_object) {
        Some(data) => CharacterData::from_object(data, true),
        None => CharacterData::from_object(obj, false),
    };
    let fallback = |field: &mut String, keys: &[&str]| {
        if field.trim().is_empty()
            && let Some(value) = keys.iter().map(|k| text(obj, k)).find(|v| !v.is_empty())
        {
            *field = value;
        }
    };
    fallback(&mut card.name, &["name", "char_name"]);
    fallback(&mut card.description, &["description", "char_persona"]);
    fallback(&mut card.personality, &["personality"]);
    fallback(&mut card.scenario, &["scenario", "world_scenario"]);
    fallback(&mut card.first_mes, &["first_mes", "char_greeting"]);
    fallback(&mut card.mes_example, &["mes_example", "example_dialogue"]);
    fallback(&mut card.creator_notes, &["creatorcomment"]);
    if card.name.trim().is_empty() {
        return Err(invalid("Character card has no name"));
    }
    Ok(card)
}

fn decode_card(encoded: &str) -> AppResult<CharacterData> {
    let bytes = B64
        .decode(encoded.trim())
        .map_err(|e| invalid(format!("Card metadata is not valid base64: {e}")))?;
    let json = String::from_utf8(bytes).map_err(|_| invalid("Card metadata is not UTF-8"))?;
    parse_card_json(&json)
}

/// Reads the card embedded in a PNG, preferring V3 and falling back to V2.
pub fn read_png_card(bytes: &[u8]) -> AppResult<CharacterData> {
    let entries = png_meta::read_text(bytes)?;
    let find = |key: &str| entries.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str());
    let v3 = find("ccv3").map(decode_card);
    match (v3, find("chara")) {
        (Some(Ok(card)), _) => Ok(card),
        (_, Some(v2)) => decode_card(v2),
        (Some(Err(e)), None) => Err(e),
        (None, None) => Err(invalid("This PNG has no embedded character card")),
    }
}

/// Embeds the card into PNG bytes as both V2 (`chara`) and V3 (`ccv3`).
pub fn write_png_card(image_png: &[u8], card: &CharacterData) -> AppResult<Vec<u8>> {
    let data = serde_json::to_value(card)?;
    let v2 = json!({
        "spec": "chara_card_v2",
        "spec_version": "2.0",
        "data": data,
        // V1 mirror for old readers, as SillyTavern writes it.
        "name": card.name,
        "description": card.description,
        "personality": card.personality,
        "scenario": card.scenario,
        "first_mes": card.first_mes,
        "mes_example": card.mes_example,
        "creatorcomment": card.creator_notes,
        "tags": card.tags,
        "fav": card.is_favorite(),
        "avatar": "none",
    });
    let mut v3_data = data;
    if let Some(obj) = v3_data.as_object_mut() {
        obj.entry("group_only_greetings").or_insert_with(|| json!([]));
    }
    let v3 = json!({ "spec": "chara_card_v3", "spec_version": "3.0", "data": v3_data });
    let chara = B64.encode(serde_json::to_string(&v2)?);
    let ccv3 = B64.encode(serde_json::to_string(&v3)?);
    png_meta::write_text(image_png, &[("chara", chara.as_str()), ("ccv3", ccv3.as_str())])
}

fn encode_png(img: &image::DynamicImage) -> AppResult<Vec<u8>> {
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)?;
    Ok(out)
}

/// Decodes any supported image and center-crops it to `width`x`height` PNG.
pub fn normalize_avatar(bytes: &[u8], width: u32, height: u32) -> AppResult<Vec<u8>> {
    let img = image::load_from_memory(bytes)?;
    encode_png(&img.resize_to_fill(width, height, FilterType::Lanczos3))
}

/// A soft gradient tinted from the name, used when a card has no picture.
pub fn default_avatar(name: &str) -> AppResult<Vec<u8>> {
    let hash = name.bytes().fold(5381u32, |h, b| h.wrapping_mul(33) ^ u32::from(b));
    let hue = (hash % 360) as f32;
    let (r, g, b) = hsl_to_rgb(hue, 0.35, 0.34);
    let img = image::RgbImage::from_fn(256, 384, |_, y| {
        let shade = 1.0 - (y as f32 / 384.0) * 0.4;
        image::Rgb([(r * shade) as u8, (g * shade) as u8, (b * shade) as u8])
    });
    encode_png(&image::DynamicImage::ImageRgb8(img))
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match h as u32 {
        0..60 => (c, x, 0.0),
        60..120 => (x, c, 0.0),
        120..180 => (0.0, c, x),
        180..240 => (0.0, x, c),
        240..300 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    ((r + m) * 255.0, (g + m) * 255.0, (b + m) * 255.0)
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct CharacterSummary {
    pub file: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub fav: bool,
    pub creator: String,
    pub modified: u64,
}

fn summarize(file: &str, card: &CharacterData, modified: u64) -> CharacterSummary {
    CharacterSummary {
        file: file.to_string(),
        name: card.name.clone(),
        description: card.description.chars().take(160).collect(),
        tags: card.tags.clone(),
        fav: card.is_favorite(),
        creator: card.creator.clone(),
        modified,
    }
}

/// Lists all cards, re-parsing only files whose modification time changed.
pub fn list(dirs: &DataDirs, cache: &mut HashMap<String, CharacterSummary>) -> AppResult<Vec<CharacterSummary>> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for entry in fs::read_dir(dirs.characters())? {
        let path = entry?.path();
        let is_png = path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("png"));
        let Some(file) = path.file_name().and_then(|n| n.to_str()).map(String::from) else { continue };
        if !is_png {
            continue;
        }
        let modified = paths::modified_ms(&path);
        seen.insert(file.clone());
        if let Some(hit) = cache.get(&file).filter(|c| c.modified == modified) {
            out.push(hit.clone());
            continue;
        }
        match fs::read(&path).map_err(AppError::from).and_then(|bytes| read_png_card(&bytes)) {
            Ok(card) => {
                let summary = summarize(&file, &card, modified);
                cache.insert(file, summary.clone());
                out.push(summary);
            }
            Err(e) => eprintln!("Skipping character {file}: {e}"),
        }
    }
    cache.retain(|k, _| seen.contains(k));
    out.sort_by(|a, b| b.fav.cmp(&a.fav).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(out)
}

pub fn load(dirs: &DataDirs, file: &str) -> AppResult<CharacterData> {
    let path = dirs.character_file(file)?;
    if !path.exists() {
        return Err(AppError::NotFound(format!("character {file}")));
    }
    read_png_card(&fs::read(path)?)
}

fn save_new(dirs: &DataDirs, card: &CharacterData, image_png: &[u8]) -> AppResult<String> {
    let stem = paths::sanitize_file_stem(&card.name);
    let file = paths::unique_file_name(&dirs.characters(), &stem, "png");
    paths::write_atomic(&dirs.characters().join(&file), &write_png_card(image_png, card)?)?;
    Ok(file)
}

pub fn create(dirs: &DataDirs, card: &CharacterData, avatar: Option<&[u8]>) -> AppResult<String> {
    if card.name.trim().is_empty() {
        return Err(invalid("Character name is required"));
    }
    let image = match avatar {
        Some(bytes) => normalize_avatar(bytes, 512, 768)?,
        None => default_avatar(&card.name)?,
    };
    save_new(dirs, card, &image)
}

/// Rewrites the card in place. The file name stays the same so chats keep their folder.
pub fn update(dirs: &DataDirs, file: &str, card: &CharacterData, avatar: Option<&[u8]>) -> AppResult<()> {
    if card.name.trim().is_empty() {
        return Err(invalid("Character name is required"));
    }
    let path = dirs.character_file(file)?;
    let image = match avatar {
        Some(bytes) => normalize_avatar(bytes, 512, 768)?,
        None => fs::read(&path)?,
    };
    paths::write_atomic(&path, &write_png_card(&image, card)?)
}

pub fn delete(dirs: &DataDirs, file: &str, delete_chats: bool) -> AppResult<()> {
    fs::remove_file(dirs.character_file(file)?)?;
    if delete_chats {
        let chats = dirs.chat_dir(file)?;
        if chats.exists() {
            fs::remove_dir_all(chats)?;
        }
    }
    Ok(())
}

/// Imports a PNG card (keeps its artwork) or a JSON card (gets a generated avatar).
pub fn import(dirs: &DataDirs, bytes: &[u8], file_name: &str) -> AppResult<String> {
    let (card, image) = if png_meta::is_png(bytes) {
        (read_png_card(bytes)?, bytes.to_vec())
    } else if bytes.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'{') {
        let raw = std::str::from_utf8(bytes).map_err(|_| invalid("JSON card is not UTF-8"))?;
        let card = parse_card_json(raw)?;
        let image = default_avatar(&card.name)?;
        (card, image)
    } else {
        return Err(invalid(format!("Unsupported file \"{file_name}\". Use a PNG or JSON character card.")));
    };
    save_new(dirs, &card, &image)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs() -> (tempfile::TempDir, DataDirs) {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = DataDirs::new(tmp.path().to_path_buf()).unwrap();
        (tmp, dirs)
    }

    #[test]
    fn parses_v1_and_pygmalion() {
        let v1 = parse_card_json(r#"{"name":"Ann","description":"d","first_mes":"hi","chat":"x","avatar":"none"}"#).unwrap();
        assert_eq!((v1.name.as_str(), v1.first_mes.as_str()), ("Ann", "hi"));
        assert!(v1.other.is_empty(), "V1 meta keys must not leak into data");
        let pyg = parse_card_json(r#"{"char_name":"Bo","char_persona":"p","char_greeting":"yo"}"#).unwrap();
        assert_eq!((pyg.name.as_str(), pyg.description.as_str(), pyg.first_mes.as_str()), ("Bo", "p", "yo"));
    }

    #[test]
    fn v2_keeps_unknown_fields_and_lenient_types() {
        let card = parse_card_json(
            r#"{"spec":"chara_card_v2","data":{"name":"Cy","tags":"a, b","nickname":"C","alternate_greetings":["g2"],"extensions":{"fav":true}}}"#,
        )
        .unwrap();
        assert_eq!(card.tags, vec!["a", "b"]);
        assert_eq!(card.other.get("nickname"), Some(&json!("C")));
        assert!(card.is_favorite());
        assert!(parse_card_json(r#"{"data":{"description":"no name"}}"#).is_err());
    }

    #[test]
    fn png_card_roundtrip_through_import() {
        let (_tmp, dirs) = dirs();
        let card = CharacterData { name: "Dee/Two".into(), first_mes: "Hello {{user}}".into(), ..Default::default() };
        let png = write_png_card(&png_meta::tiny_png(), &card).unwrap();
        let file = import(&dirs, &png, "x.png").unwrap();
        assert_eq!(file, "Dee_Two.png");
        let loaded = load(&dirs, &file).unwrap();
        assert_eq!((loaded.name.as_str(), loaded.first_mes.as_str()), ("Dee/Two", "Hello {{user}}"));
        // V3 adds group_only_greetings; it is preserved as an extra field.
        assert_eq!(loaded.other.get("group_only_greetings"), Some(&json!([])));
        let again = import(&dirs, &png, "x.png").unwrap();
        assert_eq!(again, "Dee_Two1.png");

        let mut cache = HashMap::new();
        let listed = list(&dirs, &mut cache).unwrap();
        assert_eq!(listed.len(), 2);
        delete(&dirs, &again, true).unwrap();
        assert_eq!(list(&dirs, &mut cache).unwrap().len(), 1);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn create_without_avatar_and_update() {
        let (_tmp, dirs) = dirs();
        let mut card = CharacterData { name: "Eve".into(), ..Default::default() };
        let file = create(&dirs, &card, None).unwrap();
        card.description = "changed".into();
        update(&dirs, &file, &card, None).unwrap();
        assert_eq!(load(&dirs, &file).unwrap().description, "changed");
        assert!(import(&dirs, b"plain text", "a.txt").is_err());
    }
}
