//! Release check against GitHub Releases (used on Android; desktop uses
//! tauri-plugin-updater with the `latest.json` the release workflow uploads).
//!
//! Only published, non-prerelease releases count: GitHub's `/releases/latest`
//! skips drafts and prereleases, so a `v1.2.0-beta.1` tag never prompts users.

use serde::Serialize;
use serde_json::Value;

use crate::error::AppResult;
use crate::providers::check_status;

pub const REPO: &str = "faizonly5953/sillytavern-on-rust";

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
}
