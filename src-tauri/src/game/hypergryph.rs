use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

// ─── API response types ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct GetLatestResponse {
    version: String,
    pkg: Option<PkgInfo>,
    patch: Option<PkgInfo>,
}

#[derive(Debug, Deserialize)]
struct PkgInfo {
    packs: Option<Vec<RawPack>>,
    total_size: Option<String>,
    file_path: Option<String>,
    game_files_md5: Option<String>,
    // for single-file responses
    url: Option<String>,
    md5: Option<String>,
    package_size: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawPack {
    url: String,
    md5: String,
    package_size: String,
}

// ─── Public types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GamePack {
    pub url: String,
    pub md5: String,
    pub size: u64,
    /// Filename derived from URL
    pub filename: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameManifest {
    pub game_id: String,
    pub version: String,
    pub packs: Vec<GamePack>,
    pub total_size: u64,
    pub game_files_md5: String,
    /// URL to the /files endpoint for individual asset verification
    pub file_path: String,
}

// ─── Game config registry ─────────────────────────────────────────────────────

struct GameApiConfig {
    appcode: &'static str,
    channel: u32,
    sub_channel: u32,
}

fn game_api_config(game_id: &str) -> Option<GameApiConfig> {
    match game_id {
        "arknights" => Some(GameApiConfig {
            appcode: "GzD1CpaWgmSq1wew",
            channel: 1,
            sub_channel: 1,
        }),
        "endfield" => Some(GameApiConfig {
            appcode: "6LL0KJuqHBVz33WK",
            channel: 1,
            sub_channel: 1,
        }),
        _ => None,
    }
}

// ─── API client ───────────────────────────────────────────────────────────────

const LAUNCHER_API_BASE: &str = "https://launcher.hypergryph.com/api/game";

/// Fetch the latest full-install package manifest for a game.
pub async fn fetch_game_manifest(
    game_id: &str,
    client: &reqwest::Client,
) -> Result<GameManifest> {
    let cfg = game_api_config(game_id)
        .ok_or_else(|| anyhow!("game '{}' 暂不支持下载", game_id))?;

    let url = format!(
        "{}/get_latest?appcode={}&channel={}&sub_channel={}&platform=Windows",
        LAUNCHER_API_BASE, cfg.appcode, cfg.channel, cfg.sub_channel
    );

    let resp = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await?
        .error_for_status()?;

    let data: GetLatestResponse = resp.json().await?;

    let pkg = data
        .pkg
        .ok_or_else(|| anyhow!("API 返回中没有 pkg 字段"))?;

    let raw_packs = pkg
        .packs
        .unwrap_or_default();

    let packs: Vec<GamePack> = raw_packs
        .into_iter()
        .map(|p| {
            let raw_name = p.url.rsplit('/').next().unwrap_or("unknown");
            // Strip query string (e.g. ?auth_key=... on Endfield CDN URLs)
            let filename = raw_name
                .split_once('?')
                .map(|(name, _)| name)
                .unwrap_or(raw_name)
                .to_string();
            let size = p.package_size.parse::<u64>().unwrap_or(0);
            GamePack {
                url: p.url,
                md5: p.md5,
                size,
                filename,
            }
        })
        .collect();

    let total_size = pkg
        .total_size
        .as_deref()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or_else(|| packs.iter().map(|p| p.size).sum());

    Ok(GameManifest {
        game_id: game_id.to_string(),
        version: data.version,
        packs,
        total_size,
        game_files_md5: pkg.game_files_md5.unwrap_or_default(),
        file_path: pkg.file_path.unwrap_or_default(),
    })
}

/// Fetch update patch manifest (from current version to latest).
/// Returns None if no patch is available (clean install required).
pub async fn fetch_patch_manifest(
    game_id: &str,
    current_version: &str,
    client: &reqwest::Client,
) -> Result<Option<GameManifest>> {
    let cfg = game_api_config(game_id)
        .ok_or_else(|| anyhow!("game '{}' 暂不支持", game_id))?;

    let url = format!(
        "{}/get_latest?appcode={}&channel={}&sub_channel={}&platform=Windows&current_version={}",
        LAUNCHER_API_BASE, cfg.appcode, cfg.channel, cfg.sub_channel, current_version
    );

    let resp = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await?
        .error_for_status()?;

    let data: GetLatestResponse = resp.json().await?;

    let Some(patch) = data.patch else {
        return Ok(None);
    };

    let raw_packs = patch.packs.unwrap_or_default();
    if raw_packs.is_empty() {
        return Ok(None);
    }

    let packs: Vec<GamePack> = raw_packs
        .into_iter()
        .map(|p| {
            let raw_name = p.url.rsplit('/').next().unwrap_or("unknown");
            let filename = raw_name
                .split_once('?')
                .map(|(name, _)| name)
                .unwrap_or(raw_name)
                .to_string();
            let size = p.package_size.parse::<u64>().unwrap_or(0);
            GamePack { url: p.url, md5: p.md5, size, filename }
        })
        .collect();

    let total_size = patch
        .total_size
        .as_deref()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or_else(|| packs.iter().map(|p| p.size).sum());

    Ok(Some(GameManifest {
        game_id: game_id.to_string(),
        version: data.version,
        packs,
        total_size,
        game_files_md5: patch.game_files_md5.unwrap_or_default(),
        file_path: patch.file_path.unwrap_or_default(),
    }))
}
