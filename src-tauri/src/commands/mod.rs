use crate::config::{AppConfig, AppSettings};
use crate::download::{DownloadManager, DownloadProgress, DownloadTask};
use crate::game::{self, GameManifest};
use std::collections::HashMap;
use std::sync::Arc;
use sysinfo::{Pid as SysPid, ProcessesToUpdate, System as SysInfo};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::RwLock;

pub struct AppState {
    pub download_manager: Arc<DownloadManager>,
    pub http_client: reqwest::Client,
    /// game_id → sysinfo PID of the running game process
    pub running_games: HashMap<String, SysPid>,
}

// ─── Game status event ────────────────────────────────────────────────────────

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameStatus {
    pub game_id: String,
    pub running: bool,
}

// ─── Config / Settings ───────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_app_config(
    config: State<'_, Arc<RwLock<AppConfig>>>,
) -> Result<AppConfig, String> {
    Ok(config.read().await.clone())
}

#[tauri::command]
pub async fn set_settings(
    settings: AppSettings,
    app: AppHandle,
    config: State<'_, Arc<RwLock<AppConfig>>>,
) -> Result<(), String> {
    {
        let mut c = config.write().await;
        c.settings = settings;
    }
    let c = config.read().await.clone();
    crate::config::save_config(&app, &c)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_game_path(
    game_id: String,
    path: Option<String>,
    app: AppHandle,
    config: State<'_, Arc<RwLock<AppConfig>>>,
) -> Result<(), String> {
    {
        let mut c = config.write().await;
        match path {
            Some(p) => {
                c.game_paths.insert(game_id, p);
            }
            None => {
                c.game_paths.remove(&game_id);
            }
        }
    }
    let c = config.read().await.clone();
    crate::config::save_config(&app, &c)
        .await
        .map_err(|e| e.to_string())
}

// ─── Window controls ─────────────────────────────────────────────────────────

#[tauri::command]
pub async fn window_minimize(app: AppHandle) -> Result<(), String> {
    app.get_webview_window("main")
        .ok_or("no main window")?
        .minimize()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn window_toggle_maximize(app: AppHandle) -> Result<(), String> {
    let win = app.get_webview_window("main").ok_or("no main window")?;
    let is_max = win.is_maximized().map_err(|e: tauri::Error| e.to_string())?;
    if is_max {
        win.unmaximize().map_err(|e: tauri::Error| e.to_string())
    } else {
        win.maximize().map_err(|e: tauri::Error| e.to_string())
    }
}

#[tauri::command]
pub async fn window_close(app: AppHandle) -> Result<(), String> {
    app.get_webview_window("main")
        .ok_or("no main window")?
        .close()
        .map_err(|e| e.to_string())
}

// ─── Game management ─────────────────────────────────────────────────────────

#[tauri::command]
pub async fn launch_game(
    game_id: String,
    install_path: String,
    app: AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;

    if state.read().await.running_games.contains_key(&game_id) {
        return Err("游戏已在运行中".into());
    }

    let exe_path = game::require_game_exe(&game_id, &install_path)
        .map_err(|e| e.to_string())?;

    let exe_name = exe_path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    app.opener()
        .open_path(exe_path.to_string_lossy(), None::<&str>)
        .map_err(|e| format!("无法启动 {}: {}", exe_path.display(), e))?;

    let app_clone = app.clone();
    let state_arc = Arc::clone(state.inner());
    let game_id_clone = game_id.clone();
    tauri::async_runtime::spawn(async move {
        monitor_game(app_clone, state_arc, game_id_clone, exe_name).await;
    });

    Ok(())
}

/// Background task: find the game process after launch, then watch for it to exit.
async fn monitor_game(
    app: AppHandle,
    state: Arc<RwLock<AppState>>,
    game_id: String,
    exe_name: String,
) {
    let mut sys = SysInfo::new();

    // Retry finding the process for up to 10 seconds
    let mut game_pid: Option<SysPid> = None;
    for _ in 0..20 {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        sys.refresh_processes(ProcessesToUpdate::All);
        for (pid, proc) in sys.processes() {
            let name = proc.name().to_string_lossy().to_lowercase();
            if name == exe_name || name.trim_end_matches(".exe") == exe_name.trim_end_matches(".exe") {
                game_pid = Some(*pid);
                break;
            }
        }
        if game_pid.is_some() {
            break;
        }
    }

    let Some(pid) = game_pid else {
        let _ = app.emit("game:status", GameStatus { game_id, running: false });
        return;
    };

    state.write().await.running_games.insert(game_id.clone(), pid);
    let _ = app.emit("game:status", GameStatus { game_id: game_id.clone(), running: true });

    // Poll every 2 seconds until the process exits.
    // Use ProcessesToUpdate::All so the full PID list is re-enumerated;
    // anti-cheat can block per-PID inspection but cannot hide a missing PID
    // from a full process snapshot.
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        sys.refresh_processes(ProcessesToUpdate::All);
        if sys.process(pid).is_none() {
            break;
        }
    }

    let _ = app.emit("game:status", GameStatus { game_id: game_id.clone(), running: false });
    state.write().await.running_games.remove(&game_id);
}

#[tauri::command]
pub async fn validate_game_path(game_id: String, path: String) -> bool {
    game::check_game_installed(&game_id, &path)
}

#[tauri::command]
pub async fn fetch_game_version(
    game_id: String,
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<Option<String>, String> {
    let s = state.read().await;
    game::fetch_latest_version(&game_id, &s.http_client)
        .await
        .map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
pub struct GamePathResult {
    pub path: String,
    pub installed: bool,
}

#[tauri::command]
pub async fn select_game_path(
    game_id: String,
    app: AppHandle,
) -> Result<Option<GamePathResult>, String> {
    use tauri_plugin_dialog::DialogExt;
    let picked = app
        .dialog()
        .file()
        .set_title(format!("选择 {} 安装目录", game_id))
        .blocking_pick_folder();

    let Some(file_path) = picked else {
        return Ok(None);
    };

    let path_str = file_path.to_string();
    if !game::validate_install_path(&game_id, &path_str) {
        return Err(format!("所选路径不是有效目录：{}", path_str));
    }

    let installed = game::check_game_installed(&game_id, &path_str);
    Ok(Some(GamePathResult { path: path_str, installed }))
}

#[tauri::command]
pub async fn select_download_path(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let path = app
        .dialog()
        .file()
        .set_title("选择下载目录")
        .blocking_pick_folder();
    Ok(path.map(|p| p.to_string()))
}

// ─── Game download (Hypergryph API) ──────────────────────────────────────────

/// Fetch the full-install pack manifest from Hypergryph API.
#[tauri::command]
pub async fn fetch_game_manifest(
    game_id: String,
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<GameManifest, String> {
    let s = state.read().await;
    game::fetch_game_manifest(&game_id, &s.http_client)
        .await
        .map_err(|e| e.to_string())
}

/// Start downloading all packs for a full game install.
/// Each pack becomes a separate download task; progress is emitted via events.
/// Returns a list of task IDs (one per pack).
#[tauri::command]
pub async fn start_game_install(
    game_id: String,
    dest_dir: String,
    app: AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<Vec<String>, String> {
    let manifest = {
        let s = state.read().await;
        game::fetch_game_manifest(&game_id, &s.http_client)
            .await
            .map_err(|e| e.to_string())?
    };

    let mut task_ids = Vec::with_capacity(manifest.packs.len());

    log::info!(
        "[install] game={} packs={} dest={}",
        game_id, manifest.packs.len(), dest_dir
    );

    for pack in &manifest.packs {
        let dest_path = format!("{}/{}", dest_dir.trim_end_matches('/'), pack.filename);
        log::info!("[install] pack={} size={} dest={}", pack.filename, pack.size, dest_path);
        let task_id = {
            let s = state.read().await;
            s.download_manager
                .create_task(
                    game_id.clone(),
                    pack.filename.clone(),
                    pack.url.clone(),
                    dest_path,
                    Some(pack.size),       // known from manifest — skips HEAD
                    None,
                    Some(pack.md5.clone()),
                )
                .await
                .map_err(|e| e.to_string())?
        };

        let app_clone = app.clone();
        let tid = task_id.clone();
        {
            let s = state.read().await;
            s.download_manager
                .start_task(task_id.clone(), move |progress: DownloadProgress| {
                    let _ = app_clone.emit("download:progress", &progress);
                })
                .await
                .map_err(|e| e.to_string())?;
        }

        task_ids.push(tid);
    }

    Ok(task_ids)
}

// ─── Generic download management ─────────────────────────────────────────────

#[tauri::command]
pub async fn get_download_tasks(
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<Vec<DownloadTask>, String> {
    let s = state.read().await;
    Ok(s.download_manager.get_tasks().await)
}

#[tauri::command]
pub async fn start_download_task(
    task_id: String,
    app: AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<(), String> {
    let s = state.read().await;
    let app_clone = app.clone();
    s.download_manager
        .start_task(task_id, move |progress: DownloadProgress| {
            let _ = app_clone.emit("download:progress", &progress);
        })
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn pause_download_task(
    task_id: String,
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<(), String> {
    let s = state.read().await;
    s.download_manager
        .pause_task(&task_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cancel_download_task(
    task_id: String,
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<(), String> {
    let s = state.read().await;
    s.download_manager
        .cancel_task(&task_id)
        .await
        .map_err(|e| e.to_string())
}

// ─── Cache management ─────────────────────────────────────────────────────────

/// Delete the hot-update cache directory for a game.
/// Arknights / Endfield both store cached assets in `HotUpdate/`.
#[tauri::command]
pub async fn clear_game_cache(
    game_id: String,
    install_path: String,
) -> Result<(), String> {
    let cache_dirs: &[&str] = match game_id.as_str() {
        "arknights" | "endfield" => &["HotUpdate"],
        _ => return Err(format!("未知游戏：{}", game_id)),
    };

    let base = std::path::Path::new(&install_path);
    for dir_name in cache_dirs {
        let path = base.join(dir_name);
        if path.is_dir() {
            tokio::fs::remove_dir_all(&path)
                .await
                .map_err(|e| format!("清除 {} 失败：{}", dir_name, e))?;
        }
    }
    Ok(())
}
