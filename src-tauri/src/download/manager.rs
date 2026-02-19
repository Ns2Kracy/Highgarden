use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs::{self, OpenOptions};
use tokio::io::{AsyncSeekExt, AsyncWriteExt, SeekFrom};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use uuid::Uuid;

// ─── Types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DownloadStatus {
    Pending,
    Downloading,
    Paused,
    Verifying,
    Completed,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadChunk {
    pub id: usize,
    pub url: String,
    pub start: u64,
    pub end: u64,
    pub downloaded: u64,
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadTask {
    pub id: String,
    pub game_id: String,
    pub name: String,
    pub dest_path: String,
    pub total_size: u64,
    pub downloaded_size: u64,
    pub progress: f64,
    pub speed: u64,
    pub status: DownloadStatus,
    pub error: Option<String>,
    pub created_at: u64,
    pub chunks: Vec<DownloadChunk>,
    pub sha256: Option<String>,
    pub md5: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub task_id: String,
    pub downloaded_size: u64,
    pub total_size: u64,
    pub progress: f64,
    pub speed: u64,
    pub status: DownloadStatus,
    pub error: Option<String>,
}

// ─── Download Manager ───────────────────────────────────────────────────────

pub struct DownloadManager {
    client: Client,
    tasks: Arc<RwLock<HashMap<String, DownloadTask>>>,
    handles: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
    /// Limits how many files can be actively downloading at once.
    semaphore: Arc<tokio::sync::Semaphore>,
    persist_path: Option<Arc<PathBuf>>,
}

impl DownloadManager {
    /// `max_concurrent` — how many files download simultaneously (e.g. 3).
    pub fn new(
        max_concurrent: usize,
        proxy_url: Option<&str>,
        persist_path: Option<PathBuf>,
    ) -> Result<Self> {
        let mut builder = Client::builder()
            .user_agent("Mozilla/5.0 Highgarden/0.1.0")
            .tcp_keepalive(std::time::Duration::from_secs(30))
            // Only limit the TCP connect phase; do NOT set a total request
            // timeout — that would kill body streaming for large files.
            .connect_timeout(std::time::Duration::from_secs(30));

        if let Some(proxy) = proxy_url {
            builder = builder.proxy(reqwest::Proxy::all(proxy)?);
        }

        Ok(Self {
            client: builder.build()?,
            tasks: Arc::new(RwLock::new(HashMap::new())),
            handles: Arc::new(Mutex::new(HashMap::new())),
            semaphore: Arc::new(tokio::sync::Semaphore::new(max_concurrent)),
            persist_path: persist_path.map(Arc::new),
        })
    }

    /// Load tasks saved from the previous session. Called once at startup.
    /// Tasks that were actively "downloading" are reset to "paused".
    pub async fn load_persisted(&self) -> Result<()> {
        let Some(path) = &self.persist_path else {
            return Ok(());
        };
        if !path.exists() {
            return Ok(());
        }
        let raw = fs::read_to_string(path.as_ref()).await?;
        let saved: HashMap<String, DownloadTask> = serde_json::from_str(&raw).unwrap_or_default();
        let mut tasks = self.tasks.write().await;
        for (id, mut task) in saved {
            if matches!(
                task.status,
                DownloadStatus::Downloading | DownloadStatus::Verifying
            ) {
                task.status = DownloadStatus::Paused;
                task.speed = 0;
            }
            tasks.insert(id, task);
        }
        log::info!("[dl] loaded {} persisted task(s)", tasks.len());
        Ok(())
    }

    /// Persist all non-completed tasks to disk.
    async fn persist(&self) {
        let Some(path) = &self.persist_path else {
            return;
        };
        let tasks = self.tasks.read().await;
        let to_save: HashMap<&String, &DownloadTask> = tasks
            .iter()
            .filter(|(_, t)| !matches!(t.status, DownloadStatus::Completed | DownloadStatus::Error))
            .collect();
        match serde_json::to_string_pretty(&to_save) {
            Ok(raw) => {
                if let Err(e) = fs::write(path.as_ref(), raw).await {
                    log::error!("[dl] persist write failed: {}", e);
                }
            }
            Err(e) => log::error!("[dl] persist serialize failed: {}", e),
        }
    }

    /// Create a new download task (single file, multi-chunk).
    ///
    /// If `known_size` is provided (e.g. from the API manifest) the HEAD probe
    /// is skipped entirely, which avoids issues with signed CDN URLs that do not
    /// support HEAD.
    pub async fn create_task(
        &self,
        game_id: String,
        name: String,
        url: String,
        dest_path: String,
        known_size: Option<u64>,
        sha256: Option<String>,
        md5: Option<String>,
    ) -> Result<String> {
        let (total_size, _supports_range) = if let Some(size) = known_size {
            log::info!("[dl] create_task name={name} size={size}");
            (size, true)
        } else {
            log::info!("[dl] create_task name={name} — probing HEAD {url}");
            let resp = self
                .client
                .head(&url)
                .send()
                .await
                .context("HEAD request failed")?;

            log::debug!(
                "[dl] HEAD status={} headers={:?}",
                resp.status(),
                resp.headers()
            );

            let size = resp
                .headers()
                .get(reqwest::header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);

            let range = resp
                .headers()
                .get(reqwest::header::ACCEPT_RANGES)
                .map(|v| v != "none")
                .unwrap_or(false);

            log::info!("[dl] HEAD result: size={size} supports_range={range}");
            (size, range)
        };

        // Always a single chunk — concurrency across files is controlled by
        // the semaphore in start_task, not by splitting the file.
        let chunks = vec![DownloadChunk {
            id: 0,
            url: url.clone(),
            start: 0,
            end: total_size.saturating_sub(1),
            downloaded: 0,
            completed: false,
        }];

        log::info!("[dl] create_task name={name} chunks={}", chunks.len());

        let task_id = Uuid::new_v4().to_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let task = DownloadTask {
            id: task_id.clone(),
            game_id,
            name,
            dest_path,
            total_size,
            downloaded_size: 0,
            progress: 0.0,
            speed: 0,
            status: DownloadStatus::Pending,
            error: None,
            created_at: now,
            chunks,
            sha256,
            md5,
        };

        self.tasks.write().await.insert(task_id.clone(), task);
        self.persist().await;
        Ok(task_id)
    }

    /// Start or resume a download task.
    pub async fn start_task<F>(&self, task_id: String, on_progress: F) -> Result<()>
    where
        F: Fn(DownloadProgress) + Send + Sync + 'static,
    {
        let task = {
            let mut tasks = self.tasks.write().await;
            let task = tasks
                .get_mut(&task_id)
                .ok_or_else(|| anyhow!("Task not found: {}", task_id))?;
            task.status = DownloadStatus::Downloading;

            // Resume support: use the actual file size on disk as the resume offset.
            // This is crash-safe — file bytes written are the ground truth.
            if let Ok(meta) = tokio::fs::metadata(&task.dest_path).await {
                let on_disk = meta.len();
                if on_disk > 0 && on_disk < task.total_size {
                    log::info!(
                        "[dl] resume: {} bytes already on disk for {} (total {})",
                        on_disk,
                        task.name,
                        task.total_size
                    );
                    if let Some(c) = task.chunks.get_mut(0) {
                        c.downloaded = on_disk;
                        c.completed = false;
                    }
                    task.downloaded_size = on_disk;
                    task.progress = (on_disk as f64 / task.total_size as f64 * 100.0).min(100.0);
                } else if task.total_size > 0 && on_disk >= task.total_size {
                    log::info!("[dl] file already complete for {}", task.name);
                    task.status = DownloadStatus::Completed;
                    task.progress = 100.0;
                }
            }

            task.clone()
        };

        // File was already present — nothing to download.
        if task.status == DownloadStatus::Completed {
            self.persist().await;
            return Ok(());
        }

        log::info!(
            "[dl] start_task id={} name={} dest={} size={} resume_from={}",
            task.id,
            task.name,
            task.dest_path,
            task.total_size,
            task.chunks.first().map(|c| c.downloaded).unwrap_or(0)
        );

        // Ensure destination directory exists
        if let Some(parent) = Path::new(&task.dest_path).parent() {
            fs::create_dir_all(parent).await?;
        }

        let client = self.client.clone();
        let tasks = self.tasks.clone();
        let task_id_clone = task_id.clone();
        let semaphore = self.semaphore.clone();
        let persist_path = self.persist_path.clone();

        let handle = tokio::spawn(async move {
            // Wait for a download slot.  The permit is held for the entire
            // download and dropped automatically when this block ends.
            let _permit = match semaphore.acquire().await {
                Ok(p) => p,
                Err(_) => {
                    log::error!("[dl] semaphore closed for task {}", task_id_clone);
                    return;
                }
            };
            log::info!("[dl] semaphore acquired → starting {}", task_id_clone);

            let result = Self::run_download(client, tasks.clone(), task.clone(), on_progress).await;

            let mut tasks_w = tasks.write().await;
            if let Some(t) = tasks_w.get_mut(&task_id_clone) {
                match result {
                    Ok(()) => {
                        log::info!("[dl] task {} completed: {}", task_id_clone, t.name);
                        t.status = DownloadStatus::Completed;
                        t.progress = 100.0;
                    }
                    Err(e) => {
                        log::error!("[dl] task {} FAILED: {}", task_id_clone, e);
                        t.status = DownloadStatus::Error;
                        t.error = Some(e.to_string());
                    }
                }
            }

            // Persist after completion (completed/errored tasks are dropped from the file).
            if let Some(path) = &persist_path {
                let to_save: HashMap<&String, &DownloadTask> = tasks_w
                    .iter()
                    .filter(|(_, t)| {
                        !matches!(t.status, DownloadStatus::Completed | DownloadStatus::Error)
                    })
                    .collect();
                if let Ok(raw) = serde_json::to_string_pretty(&to_save) {
                    let _ = fs::write(path.as_ref(), raw).await;
                }
            }
        });

        self.handles.lock().await.insert(task_id, handle);
        Ok(())
    }

    async fn run_download<F>(
        client: Client,
        tasks: Arc<RwLock<HashMap<String, DownloadTask>>>,
        task: DownloadTask,
        on_progress: F,
    ) -> Result<()>
    where
        F: Fn(DownloadProgress) + Send + Sync + 'static,
    {
        let pending: Vec<_> = task
            .chunks
            .iter()
            .filter(|c| !c.completed)
            .cloned()
            .collect();
        log::info!(
            "[dl] run_download task={} ({}) pending_chunks={}/{}",
            task.id,
            task.name,
            pending.len(),
            task.chunks.len()
        );

        let on_progress = Arc::new(on_progress);
        // Initialize counter from already-downloaded bytes so progress is correct on resume.
        let downloaded_counter = Arc::new(tokio::sync::Mutex::new(task.downloaded_size));
        let resume_offset = task.downloaded_size;
        let start_time = std::time::Instant::now();

        let mut join_set = tokio::task::JoinSet::new();

        for chunk in pending {
            let client = client.clone();
            let task_id = task.id.clone();
            let dest_path = task.dest_path.clone();
            let tasks = tasks.clone();
            let counter = downloaded_counter.clone();
            let total_size = task.total_size;
            let on_progress = on_progress.clone();
            let start = start_time;

            join_set.spawn(async move {
                Self::download_chunk(
                    client,
                    chunk,
                    dest_path,
                    task_id,
                    tasks,
                    counter,
                    resume_offset,
                    total_size,
                    on_progress,
                    start,
                )
                .await
            });
        }

        while let Some(result) = join_set.join_next().await {
            result.map_err(|e| anyhow!("Task join error: {}", e))??;
        }

        log::info!("[dl] run_download task={} all chunks done", task.id);

        // Verify checksum if provided
        if task.sha256.is_some() || task.md5.is_some() {
            {
                let mut tasks_w = tasks.write().await;
                if let Some(t) = tasks_w.get_mut(&task.id) {
                    t.status = DownloadStatus::Verifying;
                }
            }
            if let Some(expected_sha256) = &task.sha256 {
                log::info!("[dl] verifying sha256 for {}", task.dest_path);
                verify_sha256(&task.dest_path, expected_sha256).await?;
                log::info!("[dl] sha256 OK for {}", task.dest_path);
            } else if let Some(expected_md5) = &task.md5 {
                log::info!("[dl] verifying md5 for {}", task.dest_path);
                verify_md5(&task.dest_path, expected_md5).await?;
                log::info!("[dl] md5 OK for {}", task.dest_path);
            }
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn download_chunk<F>(
        client: Client,
        chunk: DownloadChunk,
        dest_path: String,
        task_id: String,
        tasks: Arc<RwLock<HashMap<String, DownloadTask>>>,
        downloaded_counter: Arc<tokio::sync::Mutex<u64>>,
        resume_offset: u64,
        total_size: u64,
        on_progress: Arc<F>,
        start_time: std::time::Instant,
    ) -> Result<()>
    where
        F: Fn(DownloadProgress) + Send + Sync + 'static,
    {
        let already_downloaded = chunk.downloaded;
        let range_start = chunk.start + already_downloaded;
        let range_end = chunk.end;

        if range_start > range_end {
            log::debug!("[dl] chunk {} already complete, skipping", chunk.id);
            return Ok(());
        }

        let use_range = range_start > 0;
        let mut request = client.get(&chunk.url);
        if use_range {
            request = request.header(
                reqwest::header::RANGE,
                format!("bytes={}-{}", range_start, range_end),
            );
        }

        log::info!(
            "[dl] chunk {} task={} GET {} range={}",
            chunk.id,
            task_id,
            if use_range {
                format!("{}-{}", range_start, range_end)
            } else {
                "full".to_string()
            },
            &chunk.url[..chunk.url.len().min(80)]
        );

        let response = request
            .send()
            .await
            .with_context(|| format!("chunk {} GET failed", chunk.id))?
            .error_for_status()
            .with_context(|| format!("chunk {} non-2xx status", chunk.id))?;

        let resp_status = response.status();
        let content_length = response
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("?")
            .to_string();
        log::info!(
            "[dl] chunk {} response: status={} content-length={}",
            chunk.id,
            resp_status,
            content_length
        );

        let mut stream = response.bytes_stream();

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&dest_path)
            .await
            .with_context(|| format!("open file {} failed", dest_path))?;

        file.seek(SeekFrom::Start(chunk.start + already_downloaded))
            .await
            .with_context(|| format!("seek in {} failed", dest_path))?;

        let mut chunk_downloaded = already_downloaded;
        let mut last_log_bytes = 0u64;

        while let Some(item) = stream.next().await {
            let data = item.with_context(|| format!("chunk {} stream read error", chunk.id))?;
            file.write_all(&data)
                .await
                .with_context(|| format!("chunk {} write error", chunk.id))?;

            let bytes = data.len() as u64;
            chunk_downloaded += bytes;

            let mut counter = downloaded_counter.lock().await;
            *counter += bytes;
            let total_downloaded = *counter;
            drop(counter);

            let elapsed = start_time.elapsed().as_secs_f64().max(0.001);
            // Speed reflects only bytes downloaded in this session, not the resume offset.
            let session_bytes = total_downloaded.saturating_sub(resume_offset);
            let speed = (session_bytes as f64 / elapsed) as u64;

            let progress = if total_size > 0 {
                (total_downloaded as f64 / total_size as f64 * 100.0).min(100.0)
            } else {
                0.0
            };

            // Log every 64 MB per chunk to avoid flooding
            if chunk_downloaded - last_log_bytes >= 64 * 1024 * 1024 {
                log::debug!(
                    "[dl] chunk {} +{}MB total={:.1}% speed={}/s",
                    chunk.id,
                    (chunk_downloaded - last_log_bytes) / 1024 / 1024,
                    progress,
                    format_bytes(speed)
                );
                last_log_bytes = chunk_downloaded;
            }

            on_progress(DownloadProgress {
                task_id: task_id.clone(),
                downloaded_size: total_downloaded,
                total_size,
                progress,
                speed,
                status: DownloadStatus::Downloading,
                error: None,
            });
        }

        log::info!(
            "[dl] chunk {} done downloaded={}MB",
            chunk.id,
            chunk_downloaded / 1024 / 1024
        );

        let mut tasks_w = tasks.write().await;
        if let Some(t) = tasks_w.get_mut(&task_id) {
            if let Some(c) = t.chunks.iter_mut().find(|c| c.id == chunk.id) {
                c.downloaded = chunk_downloaded;
                c.completed = true;
            }
            t.downloaded_size = t.chunks.iter().map(|c| c.downloaded).sum();
        }

        Ok(())
    }

    pub async fn pause_task(&self, task_id: &str) -> Result<()> {
        log::info!("[dl] pause_task id={}", task_id);
        if let Some(handle) = self.handles.lock().await.get(task_id) {
            handle.abort();
        }
        let mut tasks = self.tasks.write().await;
        if let Some(t) = tasks.get_mut(task_id) {
            t.status = DownloadStatus::Paused;
            t.speed = 0;
        }
        drop(tasks);
        self.persist().await;
        Ok(())
    }

    pub async fn cancel_task(&self, task_id: &str) -> Result<()> {
        log::info!("[dl] cancel_task id={}", task_id);
        if let Some(handle) = self.handles.lock().await.remove(task_id) {
            handle.abort();
        }
        let mut tasks = self.tasks.write().await;
        tasks.remove(task_id);
        drop(tasks);
        self.persist().await;
        Ok(())
    }

    pub async fn get_tasks(&self) -> Vec<DownloadTask> {
        self.tasks.read().await.values().cloned().collect()
    }

    pub async fn get_task(&self, task_id: &str) -> Option<DownloadTask> {
        self.tasks.read().await.get(task_id).cloned()
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn format_bytes(b: u64) -> String {
    if b < 1024 {
        format!("{b}B")
    } else if b < 1024 * 1024 {
        format!("{:.1}KB", b as f64 / 1024.0)
    } else if b < 1024 * 1024 * 1024 {
        format!("{:.1}MB", b as f64 / 1024.0 / 1024.0)
    } else {
        format!("{:.2}GB", b as f64 / 1024.0 / 1024.0 / 1024.0)
    }
}

// ─── Checksum verification ──────────────────────────────────────────────────

async fn verify_sha256(path: &str, expected: &str) -> Result<()> {
    let data = fs::read(path).await?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let actual = hex::encode(hasher.finalize());
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        log::error!("[dl] sha256 mismatch path={path} expected={expected} got={actual}");
        Err(anyhow!(
            "SHA256 mismatch: expected {}, got {}",
            expected,
            actual
        ))
    }
}

async fn verify_md5(path: &str, expected: &str) -> Result<()> {
    let data = fs::read(path).await?;
    let digest = md5::compute(&data);
    let actual = format!("{:x}", digest);
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        log::error!("[dl] md5 mismatch path={path} expected={expected} got={actual}");
        Err(anyhow!(
            "MD5 mismatch: expected {}, got {}",
            expected,
            actual
        ))
    }
}
