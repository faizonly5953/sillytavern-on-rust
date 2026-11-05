//! In-process app state: lives only while the app is open.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use crate::character::CharacterSummary;
use crate::error::AppResult;
use crate::paths::DataDirs;
use crate::settings::{self, Secrets, Settings};

pub struct AppState {
    pub dirs: DataDirs,
    pub http: reqwest::Client,
    pub settings: Mutex<Settings>,
    pub secrets: Mutex<Secrets>,
    pub character_cache: Mutex<HashMap<String, CharacterSummary>>,
    /// Running generations by id, so the UI can stop them.
    pub generations: Mutex<HashMap<String, tokio::task::AbortHandle>>,
    /// Avatar thumbnails keyed by path, invalidated by modification time.
    pub thumbnails: Mutex<HashMap<PathBuf, (u64, Vec<u8>)>>,
}

impl AppState {
    pub fn new(root: PathBuf) -> AppResult<Self> {
        let dirs = DataDirs::new(root)?;
        let settings = settings::load_settings(&dirs.settings_file());
        let secrets = settings::load_secrets(&dirs.secrets_file());
        let http = reqwest::Client::builder()
            .user_agent(concat!("SillyTavern-RS/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(20))
            .build()?;
        Ok(Self {
            dirs,
            http,
            settings: Mutex::new(settings),
            secrets: Mutex::new(secrets),
            character_cache: Mutex::default(),
            generations: Mutex::default(),
            thumbnails: Mutex::default(),
        })
    }
}

/// Locks without propagating poisoning; state stays usable after a panic elsewhere.
pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
