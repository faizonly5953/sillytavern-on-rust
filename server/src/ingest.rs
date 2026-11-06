//! Turns an uploaded character card into the PNG the apps store (`POST /api/v1/characters`).
//!
//! A PNG card is kept as it is (with its artwork) after checking that it really carries a card.
//! A JSON card gets a small generated avatar and is embedded in it. The server only needs the
//! card's name, for the file name; everything else in the card is stored byte for byte, so the
//! apps read it exactly as if it had been imported there.
//!
//! The rules mirror the app (`src-tauri/src/{png_meta,character,paths}.rs`). They are repeated
//! here because the server image is built from this folder alone.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Map, Value};

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
const AVATAR_WIDTH: usize = 64;
const AVATAR_HEIGHT: usize = 96;

/// An accepted card: its name (for the file name) and the PNG to store.
pub struct Card {
    pub name: String,
    pub png: Vec<u8>,
}

/// Accepts a PNG card or a JSON card (V1, Pygmalion, V2, V3).
pub fn prepare(body: &[u8]) -> Result<Card, String> {
    if body.starts_with(&SIGNATURE) {
        return from_png(body);
    }
    match std::str::from_utf8(body).map(|t| t.trim_start_matches('\u{feff}').trim()) {
        Ok(json) if json.starts_with('{') => from_json(json),
        _ => Err("Send a PNG or JSON character card as the request body".into()),
    }
}

fn from_png(png: &[u8]) -> Result<Card, String> {
    let entries = text_entries(png)?;
    let find = |key: &str| entries.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str());
    // Like the apps: V3 first, then V2.
    let name = |key: &str| find(key).and_then(decode).and_then(|card| card_name(&card));
    match name("ccv3").or_else(|| name("chara")) {
        Some(name) => Ok(Card { name, png: png.to_vec() }),
        None if find("ccv3").or_else(|| find("chara")).is_some() => {
            Err("The character data in this PNG is not a readable card with a name".into())
        }
        None => Err("This PNG has no embedded character card".into()),
    }
}

fn from_json(json: &str) -> Result<Card, String> {
    let value: Value = serde_json::from_str(json).map_err(|e| format!("The JSON card is not valid: {e}"))?;
    let name = card_name(&value).ok_or("The character card has no name")?;
    let key = if value.get("spec").and_then(Value::as_str) == Some("chara_card_v3") { "ccv3" } else { "chara" };
    Ok(Card { png: avatar_png(&name, &[(key, &B64.encode(json))]), name })
}

/// The card's name: `data.name` (V2/V3), else `name` or `char_name` (V1, Pygmalion).
fn card_name(card: &Value) -> Option<String> {
    let root = card.as_object()?;
    let text = |obj: &Map<String, Value>, key: &str| {
        obj.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(String::from)
    };
    root.get("data")
        .and_then(Value::as_object)
        .and_then(|data| text(data, "name"))
        .or_else(|| text(root, "name"))
        .or_else(|| text(root, "char_name"))
}

fn decode(encoded: &str) -> Option<Value> {
    let bytes = B64.decode(encoded.trim()).ok()?;
    let json = String::from_utf8(bytes).ok()?;
    serde_json::from_str(json.trim_start_matches('\u{feff}')).ok()
}

/// Every tEXt entry of a PNG as `(keyword, text)`; fails when the file is cut off.
fn text_entries(png: &[u8]) -> Result<Vec<(String, String)>, String> {
    let latin1 = |bytes: &[u8]| bytes.iter().map(|&b| b as char).collect::<String>();
    let mut out = Vec::new();
    let mut pos = SIGNATURE.len();
    while pos + 8 <= png.len() {
        let len = u32::from_be_bytes([png[pos], png[pos + 1], png[pos + 2], png[pos + 3]]) as usize;
        let kind = &png[pos + 4..pos + 8];
        let start = pos + 8;
        let end = start.checked_add(len).filter(|end| end + 4 <= png.len()).ok_or("The PNG file is truncated")?;
        if kind == b"tEXt"
            && let Some(nul) = png[start..end].iter().position(|&b| b == 0)
        {
            out.push((latin1(&png[start..start + nul]), latin1(&png[start + nul + 1..end])));
        }
        if kind == b"IEND" {
            return Ok(out);
        }
        pos = end + 4; // skip the CRC
    }
    Err("The PNG file is truncated".into())
}

/// A file name stem valid on Windows, Linux and Android (same rules as the app).
pub fn file_stem(name: &str) -> String {
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1",
        "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let cleaned: String = name
        .chars()
        .map(|c| if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control() { '_' } else { c })
        .collect();
    let stem: String = cleaned.trim().trim_end_matches(['.', ' ']).chars().take(100).collect();
    if stem.is_empty() || RESERVED.contains(&stem.to_ascii_uppercase().as_str()) { format!("_{stem}") } else { stem }
}

/// A soft gradient tinted from the name (the app's default avatar, only smaller) with the given
/// tEXt entries embedded.
fn avatar_png(name: &str, texts: &[(&str, &str)]) -> Vec<u8> {
    let hash = name.bytes().fold(5381u32, |h, b| h.wrapping_mul(33) ^ u32::from(b));
    let (r, g, b) = hsl_to_rgb((hash % 360) as f32, 0.35, 0.34);
    let mut raw = Vec::with_capacity(AVATAR_HEIGHT * (1 + AVATAR_WIDTH * 3));
    for y in 0..AVATAR_HEIGHT {
        let shade = 1.0 - (y as f32 / AVATAR_HEIGHT as f32) * 0.4;
        raw.push(0); // scanline filter: none
        for _ in 0..AVATAR_WIDTH {
            raw.extend([(r * shade) as u8, (g * shade) as u8, (b * shade) as u8]);
        }
    }
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend((AVATAR_WIDTH as u32).to_be_bytes());
    ihdr.extend((AVATAR_HEIGHT as u32).to_be_bytes());
    ihdr.extend([8, 2, 0, 0, 0]); // 8-bit RGB, no interlace

    let mut png = SIGNATURE.to_vec();
    write_chunk(&mut png, b"IHDR", &ihdr);
    for (key, text) in texts {
        let mut data = key.as_bytes().to_vec();
        data.push(0);
        data.extend(text.as_bytes());
        write_chunk(&mut png, b"tEXt", &data);
    }
    write_chunk(&mut png, b"IDAT", &zlib_stored(&raw));
    write_chunk(&mut png, b"IEND", &[]);
    png
}

fn write_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend((data.len() as u32).to_be_bytes());
    out.extend(kind);
    out.extend(data);
    let mut crc = crc32fast::Hasher::new();
    crc.update(kind);
    crc.update(data);
    out.extend(crc.finalize().to_be_bytes());
}

/// A zlib stream of stored (uncompressed) blocks: a tiny gradient needs no compressor.
fn zlib_stored(raw: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    let blocks = raw.chunks(65535).len();
    for (i, block) in raw.chunks(65535).enumerate() {
        let len = block.len() as u16;
        out.push(u8::from(i + 1 == blocks)); // final-block flag
        out.extend(len.to_le_bytes());
        out.extend((!len).to_le_bytes());
        out.extend(block);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in raw {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    out.extend(((b << 16) | a).to_be_bytes());
    out
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Walks the chunks, checking every CRC; returns `(kind, data)` of each.
    fn chunks(png: &[u8]) -> Vec<(String, Vec<u8>)> {
        assert!(png.starts_with(&SIGNATURE));
        let mut out = Vec::new();
        let mut pos = SIGNATURE.len();
        while pos < png.len() {
            let len = u32::from_be_bytes(png[pos..pos + 4].try_into().unwrap()) as usize;
            let kind = &png[pos + 4..pos + 8];
            let data = &png[pos + 8..pos + 8 + len];
            let crc = u32::from_be_bytes(png[pos + 8 + len..pos + 12 + len].try_into().unwrap());
            let mut hasher = crc32fast::Hasher::new();
            hasher.update(kind);
            hasher.update(data);
            assert_eq!(hasher.finalize(), crc, "bad CRC in {}", String::from_utf8_lossy(kind));
            out.push((String::from_utf8_lossy(kind).into_owned(), data.to_vec()));
            pos += 12 + len;
        }
        out
    }

    fn embedded(png: &[u8], key: &str) -> Value {
        let (_, data) = chunks(png).into_iter().find(|(k, d)| k == "tEXt" && d.starts_with(key.as_bytes())).unwrap();
        decode(std::str::from_utf8(&data[key.len() + 1..]).unwrap()).unwrap()
    }

    #[test]
    fn json_cards_get_an_avatar_and_keep_their_json() {
        let json = r#"{"name":"Mira","description":"A hunter.","first_mes":"Hi"}"#;
        let card = prepare(json.as_bytes()).unwrap();
        assert_eq!(card.name, "Mira");
        let parts = chunks(&card.png);
        assert_eq!(parts.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(), ["IHDR", "tEXt", "IDAT", "IEND"]);
        assert_eq!(&parts[0].1[..8], [0, 0, 0, AVATAR_WIDTH as u8, 0, 0, 0, AVATAR_HEIGHT as u8]);
        assert_eq!(embedded(&card.png, "chara"), serde_json::from_str::<Value>(json).unwrap());
    }

    #[test]
    fn v3_cards_are_embedded_as_ccv3_and_names_come_from_data() {
        let json = r#"{"spec":"chara_card_v3","spec_version":"3.0","name":"outer","data":{"name":"Vee"}}"#;
        let card = prepare(format!("\u{feff}  {json}\n").as_bytes()).unwrap();
        assert_eq!(card.name, "Vee");
        assert_eq!(embedded(&card.png, "ccv3")["data"]["name"], "Vee");
    }

    #[test]
    fn name_rules_follow_the_app() {
        let name = |json: &str| prepare(json.as_bytes()).map(|c| c.name);
        assert_eq!(name(r#"{"char_name":"Pyg"}"#).unwrap(), "Pyg");
        assert_eq!(name(r#"{"data":{"name":"  "},"name":"Root"}"#).unwrap(), "Root");
        assert!(name(r#"{"description":"no name"}"#).is_err());
        assert!(name(r#"{"name":"   "}"#).is_err());
        assert!(name(r#"{"name": "cut off"#).is_err());
        assert!(prepare(b"").is_err());
        assert!(prepare(b"just text").is_err());
    }

    #[test]
    fn png_cards_are_kept_byte_for_byte_after_checking() {
        let json = r#"{"spec":"chara_card_v2","data":{"name":"Rook"}}"#;
        let png = avatar_png("x", &[("chara", &B64.encode(json))]);
        let card = prepare(&png).unwrap();
        assert_eq!((card.name.as_str(), card.png == png), ("Rook", true));
        // V3 wins over V2, as in the apps.
        let both = avatar_png("x", &[("chara", &B64.encode(json)), ("ccv3", &B64.encode(r#"{"data":{"name":"Three"}}"#))]);
        assert_eq!(prepare(&both).unwrap().name, "Three");
        // A card that cannot be read is rejected, and so are plain images and cut-off files.
        assert!(prepare(&avatar_png("x", &[])).err().unwrap().contains("no embedded"));
        assert!(prepare(&avatar_png("x", &[("chara", "!!!not base64!!!")])).err().unwrap().contains("not a readable"));
        assert!(prepare(&png[..png.len() - 20]).err().unwrap().contains("truncated"));
    }

    #[test]
    fn file_stems_are_safe_everywhere() {
        assert_eq!(file_stem("Mira"), "Mira");
        assert_eq!(file_stem("A/B: C?"), "A_B_ C_");
        assert_eq!(file_stem("  trailing dots... "), "trailing dots");
        assert_eq!(file_stem("con"), "_con");
        assert_eq!(file_stem("..."), "_");
        assert_eq!(file_stem(&"x".repeat(300)).len(), 100);
    }
}
