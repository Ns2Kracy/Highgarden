use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameInfo {
    pub id: String,
    pub install_path: Option<String>,
    pub version: Option<String>,
    pub latest_version: Option<String>,
    pub update_available: bool,
}

/// Common helper / installer executables to skip during auto-detection (all lowercase).
const EXCLUDED_EXES: &[&str] = &[
    "unitycrashandler64.exe",
    "unitycrashandler32.exe",
    "crashreportclient.exe",
    "uninstall.exe",
    "uninstallgame.exe",
    "uninst.exe",
    "setup.exe",
    "vcredist_x64.exe",
    "vcredist_x86.exe",
];

/// Known exe names to try first (fast path), per game.
fn known_exe_names(game_id: &str) -> &'static [&'static str] {
    match game_id {
        "arknights" => &["Arknights.exe", "明日方舟.exe"],
        "endfield"  => &["ArknightsEndfield.exe", "EndField.exe", "Endfield.exe"],
        _ => &[],
    }
}

/// Find the game executable inside an install directory.
/// 1. Try known names first (fast).
/// 2. Fall back to the largest .exe in the directory (excluding known helpers).
fn find_game_exe(game_id: &str, install_path: &str) -> Option<std::path::PathBuf> {
    let base = Path::new(install_path);

    for name in known_exe_names(game_id) {
        let p = base.join(name);
        if p.exists() {
            return Some(p);
        }
    }

    // Scan directory: pick the largest .exe that isn't a known helper
    std::fs::read_dir(base)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let lower = name.to_string_lossy().to_lowercase();
            lower.ends_with(".exe")
                && !EXCLUDED_EXES.iter().any(|ex| lower == *ex)
        })
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            meta.is_file().then_some((e.path(), meta.len()))
        })
        .max_by_key(|(_, size)| *size)
        .map(|(path, _)| path)
}

/// Check if a directory looks like a valid install for the given game.
/// For path *selection* we only require the directory to exist.
/// For *launching* we additionally check that an executable is present.
pub fn validate_install_path(_game_id: &str, path: &str) -> bool {
    Path::new(path).is_dir()
}

/// Returns true if the game executable can be found inside the given directory.
pub fn check_game_installed(game_id: &str, install_path: &str) -> bool {
    find_game_exe(game_id, install_path).is_some()
}

/// Stricter check used before launching: returns the exe path or an error.
pub fn require_game_exe(game_id: &str, install_path: &str) -> Result<std::path::PathBuf> {
    find_game_exe(game_id, install_path).ok_or_else(|| {
        anyhow::anyhow!(
            "在 {} 中找不到 {} 可执行文件",
            install_path, game_id
        )
    })
}


// ─── Version API ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AkVersionResponse {
    #[serde(rename = "resVersion")]
    res_version: Option<String>,
    #[serde(rename = "clientVersion")]
    client_version: Option<String>,
}

/// Fetch the latest client version string for a game from Hypergryph's CDN.
pub async fn fetch_latest_version(
    game_id: &str,
    client: &reqwest::Client,
) -> Result<Option<String>> {
    let url = match game_id {
        "arknights" => "https://ak-conf.hypergryph.com/config/prod/official/Windows/version",
        "endfield" => "https://beyond-conf.hypergryph.com/config/prod/official/Windows/version",
        _ => return Ok(None),
    };

    let resp = client
        .get(url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?;

    if !resp.status().is_success() {
        return Ok(None);
    }

    let data: AkVersionResponse = resp.json().await?;
    Ok(data.client_version.or(data.res_version))
}
