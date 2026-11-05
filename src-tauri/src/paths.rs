//! Data directory layout (SillyTavern-compatible), safe file names, atomic writes.
//!
//! Layout mirrors `data/default-user/` so an existing SillyTavern folder can be copied in:
//! `characters/*.png`, `chats/<character>/*.jsonl`, `User Avatars/`, `worlds/`,
//! `settings.json`, `secrets.json`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::error::{AppResult, invalid};

pub struct DataDirs {
    root: PathBuf,
}

impl DataDirs {
    pub fn new(root: PathBuf) -> AppResult<Self> {
        let dirs = Self { root };
        for dir in [dirs.characters(), dirs.chats_root(), dirs.user_avatars(), dirs.worlds()] {
            fs::create_dir_all(dir)?;
        }
        Ok(dirs)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn characters(&self) -> PathBuf {
        self.root.join("characters")
    }

    pub fn chats_root(&self) -> PathBuf {
        self.root.join("chats")
    }

    pub fn user_avatars(&self) -> PathBuf {
        self.root.join("User Avatars")
    }

    pub fn worlds(&self) -> PathBuf {
        self.root.join("worlds")
    }

    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    pub fn secrets_file(&self) -> PathBuf {
        self.root.join("secrets.json")
    }

    pub fn character_file(&self, file: &str) -> AppResult<PathBuf> {
        Ok(self.characters().join(check_file_name(file)?))
    }

    /// SillyTavern names a character's chat folder after its avatar file without `.png`.
    pub fn chat_dir(&self, character_file: &str) -> AppResult<PathBuf> {
        let file = check_file_name(character_file)?;
        Ok(self.chats_root().join(file.strip_suffix(".png").unwrap_or(file)))
    }

    pub fn chat_file(&self, character_file: &str, chat_file: &str) -> AppResult<PathBuf> {
        Ok(self.chat_dir(character_file)?.join(check_file_name(chat_file)?))
    }
}

/// Rejects names that could escape the data directory.
pub fn check_file_name(name: &str) -> AppResult<&str> {
    let bad = name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\', '\0'])
        || name.contains(':');
    if bad { Err(invalid(format!("Invalid file name: {name}"))) } else { Ok(name) }
}

/// Turns arbitrary text into a file name stem that is valid on Windows, Linux and Android.
pub fn sanitize_file_stem(input: &str) -> String {
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let cleaned: String = input
        .chars()
        .map(|c| if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control() { '_' } else { c })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches(['.', ' ']);
    let stem: String = trimmed.chars().take(100).collect();
    if stem.is_empty() || RESERVED.contains(&stem.to_ascii_uppercase().as_str()) {
        format!("_{stem}")
    } else {
        stem
    }
}

/// First free name among `stem.ext`, `stem1.ext`, `stem2.ext`, ...
pub fn unique_file_name(dir: &Path, stem: &str, ext: &str) -> String {
    let mut name = format!("{stem}.{ext}");
    let mut counter = 1;
    while dir.join(&name).exists() {
        name = format!("{stem}{counter}.{ext}");
        counter += 1;
    }
    name
}

/// Writes through a temp file + rename so a crash never leaves a half-written file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Modification time in milliseconds since the epoch (0 when unavailable).
pub fn modified_ms(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal() {
        assert!(check_file_name("../secrets.json").is_err());
        assert!(check_file_name("a\\b.png").is_err());
        assert!(check_file_name("..").is_err());
        assert!(check_file_name("Seraphina.png").is_ok());
    }

    #[test]
    fn sanitizes_names() {
        assert_eq!(sanitize_file_stem("A/B: C?"), "A_B_ C_");
        assert_eq!(sanitize_file_stem("con"), "_con");
        assert_eq!(sanitize_file_stem("  name. "), "name");
        assert_eq!(sanitize_file_stem(""), "_");
    }

    #[test]
    fn atomic_write_replaces_existing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.json");
        write_atomic(&path, b"one").unwrap();
        write_atomic(&path, b"two").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"two");
        assert_eq!(unique_file_name(dir.path(), "x", "json"), "x1.json");
    }
}
