//! Release check against GitHub Releases (used on Android; desktop uses
//! tauri-plugin-updater with the `latest.json` the release workflow uploads).
//!
//! Only published, non-prerelease releases count: GitHub's `/releases/latest`
//! skips drafts and prereleases, so a `v1.2.0-beta.1` tag never prompts users.

use std::io::Write;
use std::path::Path;

use futures_util::StreamExt;
use serde::Serialize;
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::providers::check_status;

pub const REPO: &str = "faizonly5953/sillytavern-on-rust";

/// Only the APK of this repository's own releases is ever downloaded for installing.
pub fn is_release_asset_url(url: &str) -> bool {
    url.starts_with(&format!("https://github.com/{REPO}/releases/download/")) && url.ends_with(".apk")
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct DownloadProgress {
    pub received: u64,
    /// Unknown when the server does not send a length.
    pub total: Option<u64>,
}

/// Streams `url` into `dest`. The bytes go to `<dest>.part` and are renamed when complete, so
/// a half-finished or cut-short download is never left where the installer would pick it up.
pub async fn download<F: FnMut(DownloadProgress)>(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    mut on_progress: F,
) -> AppResult<()> {
    let resp = check_status(client.get(url).send().await?).await?;
    let total = resp.content_length();
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let part = dest.with_extension("apk.part");
    let outcome = async {
        let mut file = std::fs::File::create(&part)?;
        let mut stream = resp.bytes_stream();
        let (mut received, mut reported) = (0u64, 0u64);
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write_all(&chunk)?;
            received += chunk.len() as u64;
            // About 100 updates per download, not one per network chunk.
            if received - reported > total.unwrap_or(1 << 20) / 100 {
                reported = received;
                on_progress(DownloadProgress { received, total });
            }
        }
        file.flush()?;
        if total.is_some_and(|t| t != received) {
            return Err(AppError::Invalid("The download was cut short. Check the connection and try again.".into()));
        }
        on_progress(DownloadProgress { received, total });
        Ok(())
    }
    .await;
    match outcome {
        Ok(()) => Ok(std::fs::rename(&part, dest)?),
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            Err(e)
        }
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ReleaseInfo {
    pub version: String,
    pub current: String,
    pub notes: String,
    pub page_url: String,
    /// Direct download of the Android APK, when the release has one.
    pub apk_url: Option<String>,
    pub published_at: String,
}

/// Parses `v1.2.3` / `1.2.3-beta` into numeric parts; pre-release suffixes are ignored.
fn parse_version(raw: &str) -> Option<(u64, u64, u64)> {
    let core = raw.trim().trim_start_matches(['v', 'V']).split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    Some((parts.next()??, parts.next().flatten().unwrap_or(0), parts.next().flatten().unwrap_or(0)))
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// Extracts update info from a GitHub release JSON when it is newer than `current`.
pub fn release_info(release: &Value, current: &str) -> Option<ReleaseInfo> {
    let tag = release.get("tag_name")?.as_str()?;
    if !is_newer(tag, current) {
        return None;
    }
    let text = |key: &str| release.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
    let apk_url = release
        .get("assets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|a| a.get("name").and_then(Value::as_str).is_some_and(|n| n.ends_with(".apk")))
        .find_map(|a| a.get("browser_download_url").and_then(Value::as_str).map(String::from));
    Some(ReleaseInfo {
        version: tag.trim_start_matches(['v', 'V']).to_string(),
        current: current.to_string(),
        notes: text("body"),
        page_url: text("html_url"),
        apk_url,
        published_at: text("published_at"),
    })
}

pub async fn check_latest(client: &reqwest::Client, current: &str) -> AppResult<Option<ReleaseInfo>> {
    let resp = client
        .get(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None); // no published release yet
    }
    let release: Value = check_status(resp).await?.json().await?;
    Ok(release_info(&release, current))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn compares_versions() {
        assert!(is_newer("v0.2.0", "0.1.9"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(is_newer("v0.1.10", "0.1.9"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
        assert!(!is_newer("v0.2.0-beta.1", "0.2.0"));
        assert!(!is_newer("garbage", "0.1.0"));
    }

    #[test]
    fn picks_apk_asset() {
        let release = json!({
            "tag_name": "v0.2.0",
            "body": "Changes",
            "html_url": "https://github.com/x/y/releases/tag/v0.2.0",
            "published_at": "2026-11-06T00:00:00Z",
            "assets": [
                { "name": "latest.json", "browser_download_url": "https://a/latest.json" },
                { "name": "SillyTavernRS-0.2.0-android-arm64.apk", "browser_download_url": "https://a/app.apk" }
            ]
        });
        let info = release_info(&release, "0.1.0").unwrap();
        assert_eq!((info.version.as_str(), info.apk_url.as_deref()), ("0.2.0", Some("https://a/app.apk")));
        assert!(release_info(&release, "0.2.0").is_none());
    }

    #[test]
    fn only_official_apk_urls_are_downloaded() {
        let ok = format!("https://github.com/{REPO}/releases/download/v0.3.3/app.apk");
        assert!(is_release_asset_url(&ok));
        assert!(!is_release_asset_url(&ok.replace("github.com", "evil.example")));
        assert!(!is_release_asset_url(&ok.replace("https:", "http:")));
        assert!(!is_release_asset_url(&ok.replace(".apk", ".zip")));
        assert!(!is_release_asset_url("https://github.com/someone/else/releases/download/v1/app.apk"));
    }

    /// One-shot HTTP server that claims `claimed` bytes but only sends `body`.
    fn serve(body: Vec<u8>, claimed: usize) -> String {
        use std::io::Read;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let _ = sock.read(&mut [0u8; 2048]);
            let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {claimed}\r\nConnection: close\r\n\r\n");
            let _ = sock.write_all(head.as_bytes());
            let _ = sock.write_all(&body);
        });
        format!("http://{addr}/app.apk")
    }

    #[tokio::test]
    async fn download_reports_progress_and_renames_when_done() {
        let body: Vec<u8> = (0..50_000u32).map(|i| (i % 251) as u8).collect();
        let url = serve(body.clone(), body.len());
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("sub/update.apk");
        let mut last = None;
        download(&reqwest::Client::new(), &url, &dest, |p| last = Some(p)).await.unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), body);
        assert_eq!(last, Some(DownloadProgress { received: 50_000, total: Some(50_000) }));
        assert!(!dest.with_extension("apk.part").exists());
    }

    #[tokio::test]
    async fn cut_short_download_leaves_nothing_to_install() {
        let url = serve(vec![1; 1000], 5000);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("update.apk");
        assert!(download(&reqwest::Client::new(), &url, &dest, |_| {}).await.is_err());
        assert!(!dest.exists() && !dest.with_extension("apk.part").exists());
    }
}
