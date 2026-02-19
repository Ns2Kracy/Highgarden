mod commands;
mod config;
mod download;
mod gacha;
mod game;

use commands::{AppState, *};
use download::DownloadManager;
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::RwLock;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            // Load persisted config (blocking is fine at startup)
            let cfg =
                tauri::async_runtime::block_on(config::load_config(app.handle()))
                    .unwrap_or_default();
            let config_state: Arc<RwLock<config::AppConfig>> =
                Arc::new(RwLock::new(cfg));
            app.manage(config_state);

            let download_manager = {
                let persist_path = app
                    .path()
                    .app_data_dir()
                    .map(|d| d.join("downloads.json"))
                    .ok();
                let dm = DownloadManager::new(3, None, persist_path)
                    .expect("Failed to create download manager");
                tauri::async_runtime::block_on(dm.load_persisted())
                    .unwrap_or_else(|e| log::error!("Failed to load persisted downloads: {e}"));
                dm
            };

            let http_client = reqwest::Client::builder()
                .user_agent("Mozilla/5.0 Highgarden/0.1.0")
                .build()
                .expect("Failed to create HTTP client");

            let state = Arc::new(RwLock::new(AppState {
                download_manager: Arc::new(download_manager),
                http_client,
                running_games: std::collections::HashMap::new(),
            }));

            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Window
            window_minimize,
            window_toggle_maximize,
            window_close,
            // Config
            get_app_config,
            set_settings,
            set_game_path,
            // Game
            launch_game,
            validate_game_path,
            fetch_game_version,
            select_game_path,
            select_download_path,
            // Game download
            fetch_game_manifest,
            start_game_install,
            // Download tasks
            get_download_tasks,
            start_download_task,
            pause_download_task,
            cancel_download_task,
            // Cache
            clear_game_cache,
            // Version / update
            check_game_update,
            fetch_update_manifest,
            // Extraction
            extract_game_packs,
            // Gacha analysis
            scan_gacha_url,
            fetch_gacha_records,
            get_local_gacha_records,
            get_gacha_stats,
            export_gacha_records,
            select_gacha_export_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running highgarden");
}
