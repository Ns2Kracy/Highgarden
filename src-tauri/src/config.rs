use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tauri::Manager;
use tokio::fs;

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub theme: String,
    pub language: String,
    pub download_path: String,
    pub download_concurrency: usize,
    pub download_threads: usize,
    pub proxy_url: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            language: "zh-CN".to_string(),
            download_path: String::new(),
            download_concurrency: 8,
            download_threads: 3,
            proxy_url: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    #[serde(default)]
    pub settings: AppSettings,
    /// game_id → install path
    #[serde(default)]
    pub game_paths: HashMap<String, String>,
}

// ─── Persistence ──────────────────────────────────────────────────────────────

fn config_path(app: &tauri::AppHandle) -> Result<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| anyhow::anyhow!("Cannot resolve app data dir: {}", e))?;
    Ok(dir.join("config.json"))
}

pub async fn load_config(app: &tauri::AppHandle) -> Result<AppConfig> {
    let path = config_path(app)?;
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let raw = fs::read_to_string(&path).await?;
    let config: AppConfig = serde_json::from_str(&raw).unwrap_or_default();
    Ok(config)
}

pub async fn save_config(app: &tauri::AppHandle, config: &AppConfig) -> Result<()> {
    let path = config_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    let raw = serde_json::to_string_pretty(config)?;
    fs::write(&path, raw).await?;
    Ok(())
}
