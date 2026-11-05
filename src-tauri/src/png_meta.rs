//! PNG tEXt chunk reader/writer used to embed character cards in avatars.
//!
//! Works on raw chunks (no pixel decoding), so cards of any size load quickly.

use crate::error::{AppResult, invalid};

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

pub fn is_png(bytes: &[u8]) -> bool {
    bytes.starts_with(&SIGNATURE)
}

struct Chunk<'a> {
    kind: [u8; 4],
    data: &'a [u8],
}

fn parse_chunks(bytes: &[u8]) -> AppResult<Vec<Chunk<'_>>> {
    if !is_png(bytes) {
        return Err(invalid("Not a PNG file"));
    }
    let mut chunks = Vec::new();
    let mut pos = SIGNATURE.len();
    while pos + 8 <= bytes.len() {
        let len = u32::from_be_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]) as usize;
        let kind = [bytes[pos + 4], bytes[pos + 5], bytes[pos + 6], bytes[pos + 7]];
        let start = pos + 8;
        let end = start
            .checked_add(len)
            .filter(|end| end + 4 <= bytes.len())
            .ok_or_else(|| invalid("PNG file is truncated"))?;
        chunks.push(Chunk { kind, data: &bytes[start..end] });
        pos = end + 4; // skip CRC
        if &kind == b"IEND" {
            break;
        }
    }
    if !matches!(chunks.last(), Some(c) if &c.kind == b"IEND") {
        return Err(invalid("PNG file has no IEND chunk"));
    }
    Ok(chunks)
}

fn write_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = crc32fast::Hasher::new();
    crc.update(kind);
    crc.update(data);
    out.extend_from_slice(&crc.finalize().to_be_bytes());
}

/// tEXt is Latin-1: keyword, NUL, text.
fn split_text(data: &[u8]) -> Option<(String, String)> {
    let nul = data.iter().position(|&b| b == 0)?;
    let latin1 = |bytes: &[u8]| bytes.iter().map(|&b| b as char).collect::<String>();
    Some((latin1(&data[..nul]), latin1(&data[nul + 1..])))
}

/// Every tEXt entry as `(keyword, text)`.
pub fn read_text(bytes: &[u8]) -> AppResult<Vec<(String, String)>> {
    Ok(parse_chunks(bytes)?
        .iter()
        .filter(|c| &c.kind == b"tEXt")
        .filter_map(|c| split_text(c.data))
        .collect())
}

/// Replaces tEXt entries whose keyword matches one of `entries` (case-insensitive),
/// inserting the new ones right before IEND. Values must be ASCII (we store base64).
pub fn write_text(bytes: &[u8], entries: &[(&str, &str)]) -> AppResult<Vec<u8>> {
    let chunks = parse_chunks(bytes)?;
    let extra: usize = entries.iter().map(|(k, v)| k.len() + v.len() + 13).sum();
    let mut out = Vec::with_capacity(bytes.len() + extra);
    out.extend_from_slice(&SIGNATURE);
    for chunk in &chunks {
        if &chunk.kind == b"tEXt" {
            let replaced = split_text(chunk.data)
                .is_some_and(|(key, _)| entries.iter().any(|(k, _)| k.eq_ignore_ascii_case(&key)));
            if replaced {
                continue;
            }
        }
        if &chunk.kind == b"IEND" {
            for (key, text) in entries {
                let mut data = Vec::with_capacity(key.len() + 1 + text.len());
                data.extend_from_slice(key.as_bytes());
                data.push(0);
                data.extend_from_slice(text.as_bytes());
                write_chunk(&mut out, b"tEXt", &data);
            }
        }
        write_chunk(&mut out, &chunk.kind, chunk.data);
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) fn tiny_png() -> Vec<u8> {
    let img = image::RgbImage::from_pixel(2, 2, image::Rgb([10, 20, 30]));
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_replace() {
        let png = tiny_png();
        let once = write_text(&png, &[("chara", "AAA"), ("ccv3", "BBB")]).unwrap();
        let twice = write_text(&once, &[("Chara", "CCC")]).unwrap();
        let entries = read_text(&twice).unwrap();
        assert_eq!(entries, vec![("ccv3".to_string(), "BBB".to_string()), ("Chara".to_string(), "CCC".to_string())]);
        // Still a decodable image.
        image::load_from_memory(&twice).unwrap();
    }

    #[test]
    fn rejects_non_png() {
        assert!(read_text(b"GIF89a....").is_err());
        assert!(read_text(&SIGNATURE).is_err());
    }
}
