use crate::config::{AppConfig, AppSettings};
use crate::download::{DownloadManager, DownloadProgress, DownloadStatus, DownloadTask};
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
    let is_max = win
        .is_maximized()
        .map_err(|e: tauri::Error| e.to_string())?;
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

    let exe_path = game::require_game_exe(&game_id, &install_path).map_err(|e| e.to_string())?;

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
            if name == exe_name
                || name.trim_end_matches(".exe") == exe_name.trim_end_matches(".exe")
            {
                game_pid = Some(*pid);
                break;
            }
        }
        if game_pid.is_some() {
            break;
        }
    }

    let Some(pid) = game_pid else {
        let _ = app.emit(
            "game:status",
            GameStatus {
                game_id,
                running: false,
            },
        );
        return;
    };

    state
        .write()
        .await
        .running_games
        .insert(game_id.clone(), pid);
    let _ = app.emit(
        "game:status",
        GameStatus {
            game_id: game_id.clone(),
            running: true,
        },
    );

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

    let _ = app.emit(
        "game:status",
        GameStatus {
            game_id: game_id.clone(),
            running: false,
        },
    );
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
    Ok(Some(GamePathResult {
        path: path_str,
        installed,
    }))
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
        game_id,
        manifest.packs.len(),
        dest_dir
    );

    for pack in &manifest.packs {
        let dest_path = format!("{}/{}", dest_dir.trim_end_matches('/'), pack.filename);
        log::info!(
            "[install] pack={} size={} dest={}",
            pack.filename,
            pack.size,
            dest_path
        );
        let task_id = {
            let s = state.read().await;
            s.download_manager
                .create_task(
                    game_id.clone(),
                    pack.filename.clone(),
                    pack.url.clone(),
                    dest_path,
                    Some(pack.size), // known from manifest — skips HEAD
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
pub async fn clear_game_cache(game_id: String, install_path: String) -> Result<(), String> {
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

// ─── Version / update check ───────────────────────────────────────────────────

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckUpdateResult {
    pub local_version: Option<String>,
    pub latest_version: Option<String>,
    pub update_available: bool,
}

/// Compare the installed game version against the latest available on Hypergryph's CDN.
#[tauri::command]
pub async fn check_game_update(
    game_id: String,
    install_path: String,
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<CheckUpdateResult, String> {
    let local = game::read_local_version(&install_path);
    let s = state.read().await;
    let latest = game::fetch_latest_version(&game_id, &s.http_client)
        .await
        .map_err(|e| e.to_string())?;
    let update_available = match (&local, &latest) {
        (Some(l), Some(r)) => l != r,
        _ => false,
    };
    Ok(CheckUpdateResult {
        local_version: local,
        latest_version: latest,
        update_available,
    })
}

/// Fetch the incremental patch manifest from the current version to the latest.
/// Returns `None` if a patch is unavailable (clean install required).
#[tauri::command]
pub async fn fetch_update_manifest(
    game_id: String,
    current_version: String,
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<Option<GameManifest>, String> {
    let s = state.read().await;
    game::fetch_patch_manifest(&game_id, &current_version, &s.http_client)
        .await
        .map_err(|e| e.to_string())
}

// ─── ZIP extraction ───────────────────────────────────────────────────────────

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionProgress {
    pub game_id: String,
    /// 1-based index of the pack just completed.
    pub pack_index: usize,
    pub total_packs: usize,
    /// true when all packs are done.
    pub done: bool,
    pub error: Option<String>,
}

/// Extract all completed download packs for a game, then remove the zip files.
/// Emits `extract:progress` events as each pack finishes.
#[tauri::command]
pub async fn extract_game_packs(
    game_id: String,
    app: AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<(), String> {
    let tasks: Vec<DownloadTask> = {
        let s = state.read().await;
        s.download_manager
            .get_tasks()
            .await
            .into_iter()
            .filter(|t| t.game_id == game_id && t.status == DownloadStatus::Completed)
            .collect()
    };

    if tasks.is_empty() {
        return Err("没有可解压的已完成下载".into());
    }

    let total_packs = tasks.len();
    let game_id_clone = game_id.clone();

    tokio::task::spawn_blocking(move || {
        for (i, task) in tasks.iter().enumerate() {
            let dest_dir = std::path::Path::new(&task.dest_path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            log::info!("[extract] {}/{} — {}", i + 1, total_packs, task.name);

            match extract_zip_sync(&task.dest_path, &dest_dir) {
                Ok(()) => {
                    let _ = app.emit(
                        "extract:progress",
                        ExtractionProgress {
                            game_id: game_id_clone.clone(),
                            pack_index: i + 1,
                            total_packs,
                            done: i + 1 == total_packs,
                            error: None,
                        },
                    );
                }
                Err(e) => {
                    log::error!("[extract] failed {}: {}", task.name, e);
                    let _ = app.emit(
                        "extract:progress",
                        ExtractionProgress {
                            game_id: game_id_clone.clone(),
                            pack_index: i + 1,
                            total_packs,
                            done: false,
                            error: Some(e.to_string()),
                        },
                    );
                    return Err(format!("解压 {} 失败：{}", task.name, e));
                }
            }
        }
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("解压线程崩溃：{e}"))??;

    Ok(())
}

// ─── Gacha analysis ───────────────────────────────────────────────────────────

use crate::gacha::GachaManager;

#[tauri::command]
pub async fn scan_gacha_url(
    game_id: String,
    install_path: String,
    app: AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<Option<String>, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let client = state.read().await.http_client.clone();
    let mgr = GachaManager::new(data_dir, client);
    Ok(mgr.scan_gacha_url(&game_id, &install_path))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchGachaResult {
    pub uid: String,
    pub total: usize,
}

#[tauri::command]
pub async fn fetch_gacha_records(
    game_id: String,
    url: String,
    app: AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<FetchGachaResult, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let client = state.read().await.http_client.clone();
    let mgr = GachaManager::new(data_dir, client);

    let (uid, records) = mgr
        .fetch_all_records(&game_id, &url)
        .await
        .map_err(|e| e.to_string())?;

    let total = records.len();
    let fetched_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let data = crate::gacha::GachaData {
        uid: uid.clone(),
        game_id,
        records,
        fetched_at,
    };

    mgr.save_data(&data).map_err(|e| e.to_string())?;
    Ok(FetchGachaResult { uid, total })
}

#[tauri::command]
pub async fn get_local_gacha_records(
    game_id: String,
    app: AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<Option<crate::gacha::GachaData>, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let client = state.read().await.http_client.clone();
    let mgr = GachaManager::new(data_dir, client);
    Ok(mgr.load_data(&game_id))
}

#[tauri::command]
pub async fn get_gacha_stats(
    game_id: String,
    app: AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<Option<crate::gacha::GachaStatsResult>, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let client = state.read().await.http_client.clone();
    let mgr = GachaManager::new(data_dir, client);
    Ok(mgr.load_data(&game_id).map(|d| GachaManager::compute_stats(&d)))
}

#[tauri::command]
pub async fn export_gacha_records(
    game_id: String,
    format: String,
    dest_path: String,
    app: AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<(), String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let client = state.read().await.http_client.clone();
    let mgr = GachaManager::new(data_dir, client);

    let data = mgr
        .load_data(&game_id)
        .ok_or_else(|| "没有可导出的记录".to_string())?;

    match format.as_str() {
        "json" => GachaManager::export_json(&data.records, &dest_path).map_err(|e| e.to_string()),
        "csv" => GachaManager::export_csv(&data.records, &dest_path).map_err(|e| e.to_string()),
        "xlsx" => {
            GachaManager::export_xlsx(&data.records, &dest_path).map_err(|e| e.to_string())
        }
        _ => Err(format!("不支持的导出格式：{format}")),
    }
}

#[tauri::command]
pub async fn select_gacha_export_path(
    app: AppHandle,
    format: String,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let (extension, desc): (&str, &str) = match format.as_str() {
        "json" => ("json", "JSON 文件"),
        "csv" => ("csv", "CSV 文件"),
        "xlsx" => ("xlsx", "Excel 文件"),
        _ => return Err(format!("未知格式：{format}")),
    };
    let path = app
        .dialog()
        .file()
        .set_title(format!("导出寻访记录为 {}", desc.to_uppercase()))
        .add_filter(desc, &[extension])
        .blocking_save_file();
    Ok(path.map(|p| p.to_string()))
}

// ─── Hypergryph account auth ──────────────────────────────────────────────────

use crate::gacha::auth;
use crate::config::HypergryphSession;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HyperSessionInfo {
    pub phone_masked: String,
    pub uid: String,
}

/// Return the currently stored session (if any), without the raw token.
#[tauri::command]
pub async fn get_hypergryph_session(
    config: State<'_, Arc<RwLock<crate::config::AppConfig>>>,
) -> Result<Option<HyperSessionInfo>, String> {
    let c = config.read().await;
    Ok(c.hypergryph_session.as_ref().map(|s| HyperSessionInfo {
        phone_masked: s.phone_masked.clone(),
        uid: s.uid.clone(),
    }))
}

/// Login with phone + password, persist session token.
#[tauri::command]
pub async fn hypergryph_login_password(
    phone: String,
    password: String,
    app: AppHandle,
    config: State<'_, Arc<RwLock<crate::config::AppConfig>>>,
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<HyperSessionInfo, String> {
    let client = state.read().await.http_client.clone();
    let (uid, token, _token_type) = auth::login_by_password(&phone, &password, &client)
        .await
        .map_err(|e| e.to_string())?;

    let session = HypergryphSession {
        phone_masked: auth::mask_phone(&phone),
        uid: uid.clone(),
        token,
    };
    let info = HyperSessionInfo {
        phone_masked: session.phone_masked.clone(),
        uid: uid.clone(),
    };

    {
        let mut c = config.write().await;
        c.hypergryph_session = Some(session);
    }
    let c = config.read().await.clone();
    crate::config::save_config(&app, &c)
        .await
        .map_err(|e| e.to_string())?;

    Ok(info)
}

/// Send an SMS verification code to the given phone number.
#[tauri::command]
pub async fn hypergryph_send_sms(
    phone: String,
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<(), String> {
    let client = state.read().await.http_client.clone();
    auth::send_sms_code(&phone, &client)
        .await
        .map_err(|e| e.to_string())
}

/// Login with phone + SMS code, persist session token.
#[tauri::command]
pub async fn hypergryph_login_by_code(
    phone: String,
    code: String,
    app: AppHandle,
    config: State<'_, Arc<RwLock<crate::config::AppConfig>>>,
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<HyperSessionInfo, String> {
    let client = state.read().await.http_client.clone();
    let (uid, token, _) = auth::login_by_code(&phone, &code, &client)
        .await
        .map_err(|e| e.to_string())?;

    let session = HypergryphSession {
        phone_masked: auth::mask_phone(&phone),
        uid: uid.clone(),
        token,
    };
    let info = HyperSessionInfo {
        phone_masked: session.phone_masked.clone(),
        uid: uid.clone(),
    };

    {
        let mut c = config.write().await;
        c.hypergryph_session = Some(session);
    }
    let c = config.read().await.clone();
    crate::config::save_config(&app, &c)
        .await
        .map_err(|e| e.to_string())?;

    Ok(info)
}

/// Clear the stored session (logout).
#[tauri::command]
pub async fn hypergryph_logout(
    app: AppHandle,
    config: State<'_, Arc<RwLock<crate::config::AppConfig>>>,
) -> Result<(), String> {
    {
        let mut c = config.write().await;
        c.hypergryph_session = None;
    }
    let c = config.read().await.clone();
    crate::config::save_config(&app, &c)
        .await
        .map_err(|e| e.to_string())
}

/// One-shot command: exchange the stored auth token for a game grant,
/// then fetch and persist all gacha records. Returns fetch stats.
#[tauri::command]
pub async fn fetch_gacha_with_login(
    game_id: String,
    app: AppHandle,
    config: State<'_, Arc<RwLock<crate::config::AppConfig>>>,
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<FetchGachaResult, String> {
    let (uid, auth_token) = {
        let c = config.read().await;
        let s = c
            .hypergryph_session
            .as_ref()
            .ok_or("未登录鹰角账号，请先登录")?;
        (s.uid.clone(), s.token.clone())
    };

    let client = state.read().await.http_client.clone();

    // Get a fresh game-specific grant token
    let grant = auth::get_game_grant(&game_id, &auth_token, &client)
        .await
        .map_err(|e| format!("获取游戏授权失败（登录可能已过期）：{e}"))?;

    let gacha_url = auth::build_gacha_url(&game_id, &grant, &uid);

    // Fetch all records
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let mgr = GachaManager::new(data_dir, client);

    let (fetched_uid, records) = mgr
        .fetch_all_records(&game_id, &gacha_url)
        .await
        .map_err(|e| e.to_string())?;

    let total = records.len();
    let fetched_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    mgr.save_data(&crate::gacha::GachaData {
        uid: fetched_uid.clone(),
        game_id,
        records,
        fetched_at,
    })
    .map_err(|e| e.to_string())?;

    Ok(FetchGachaResult {
        uid: fetched_uid,
        total,
    })
}

/// Synchronously extract a zip archive into `dest_dir` and delete the archive on success.
fn extract_zip_sync(zip_path: &str, dest_dir: &str) -> anyhow::Result<()> {
    use std::io;
    use zip::ZipArchive;

    let file = std::fs::File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let out_path = match entry.enclosed_name() {
            Some(p) => std::path::Path::new(dest_dir).join(p),
            None => continue, // skip entries with unsafe paths
        };

        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out_file = std::fs::File::create(&out_path)?;
            io::copy(&mut entry, &mut out_file)?;
        }
    }

    // Remove the zip to free space after successful extraction.
    std::fs::remove_file(zip_path)?;
    log::info!("[extract] removed {}", zip_path);
    Ok(())
}
