use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;
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
pub struct DownloadChunk {
    pub id: usize,
    pub url: String,
    pub start: u64,
    pub end: u64,
    pub downloaded: u64,
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    concurrency: usize,
}

impl DownloadManager {
    pub fn new(concurrency: usize, proxy_url: Option<&str>) -> Result<Self> {
        let mut builder = Client::builder()
            .user_agent("Mozilla/5.0 Highgarden/0.1.0")
            .tcp_keepalive(std::time::Duration::from_secs(30))
            .timeout(std::time::Duration::from_secs(30));

        if let Some(proxy) = proxy_url {
            builder = builder.proxy(reqwest::Proxy::all(proxy)?);
        }

        Ok(Self {
            client: builder.build()?,
            tasks: Arc::new(RwLock::new(HashMap::new())),
            handles: Arc::new(Mutex::new(HashMap::new())),
            concurrency,
        })
    }

    /// Create a new download task (single file, multi-chunk).
    pub async fn create_task(
        &self,
        game_id: String,
        name: String,
        url: String,
        dest_path: String,
        sha256: Option<String>,
        md5: Option<String>,
    ) -> Result<String> {
        // Get file size via HEAD
        let resp = self
            .client
            .head(&url)
            .send()
            .await
            .context("HEAD request failed")?;

        let total_size = resp
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        let supports_range = resp
            .headers()
            .get(reqwest::header::ACCEPT_RANGES)
            .map(|v| v != "none")
            .unwrap_or(false);

        let chunks = if supports_range && total_size > 0 {
            self.split_into_chunks(&url, total_size)
        } else {
            // No range support — single chunk
            vec![DownloadChunk {
                id: 0,
                url: url.clone(),
                start: 0,
                end: total_size.saturating_sub(1),
                downloaded: 0,
                completed: false,
            }]
        };

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
        Ok(task_id)
    }

    fn split_into_chunks(&self, url: &str, total_size: u64) -> Vec<DownloadChunk> {
        let chunk_count = self.concurrency as u64;
        let chunk_size = total_size.div_ceil(chunk_count);
        (0..chunk_count)
            .map(|i| {
                let start = i * chunk_size;
                let end = ((i + 1) * chunk_size).min(total_size) - 1;
                DownloadChunk {
                    id: i as usize,
                    url: url.to_string(),
                    start,
                    end,
                    downloaded: 0,
                    completed: false,
                }
            })
            .collect()
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
            task.clone()
        };

        // Ensure destination directory exists
        if let Some(parent) = Path::new(&task.dest_path).parent() {
            fs::create_dir_all(parent).await?;
        }

        // Pre-allocate file if we know the size
        if task.total_size > 0 {
            let file = OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&task.dest_path)
                .await?;
            file.set_len(task.total_size).await?;
            drop(file);
        }

        let client = self.client.clone();
        let tasks = self.tasks.clone();
        let task_id_clone = task_id.clone();

        let handle = tokio::spawn(async move {
            let result = Self::run_download(client, tasks.clone(), task.clone(), on_progress).await;

            // Update final status
            let mut tasks = tasks.write().await;
            if let Some(t) = tasks.get_mut(&task_id_clone) {
                match result {
                    Ok(()) => {
                        t.status = DownloadStatus::Completed;
                        t.progress = 100.0;
                    }
                    Err(e) => {
                        t.status = DownloadStatus::Error;
                        t.error = Some(e.to_string());
                    }
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
        let on_progress = Arc::new(on_progress);
        let downloaded_counter = Arc::new(tokio::sync::Mutex::new(0u64));
        let start_time = std::time::Instant::now();
        let last_bytes = Arc::new(tokio::sync::Mutex::new(0u64));

        // Spawn a chunk downloader per chunk concurrently
        let mut join_set = tokio::task::JoinSet::new();

        for chunk in task.chunks.iter().filter(|c| !c.completed) {
            let client = client.clone();
            let task_id = task.id.clone();
            let dest_path = task.dest_path.clone();
            let tasks = tasks.clone();
            let counter = downloaded_counter.clone();
            let last = last_bytes.clone();
            let total_size = task.total_size;
            let on_progress = on_progress.clone();
            let chunk = chunk.clone();
            let start = start_time;

            join_set.spawn(async move {
                Self::download_chunk(
                    client,
                    chunk,
                    dest_path,
                    task_id,
                    tasks,
                    counter,
                    last,
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

        // Verify checksum if provided
        if task.sha256.is_some() || task.md5.is_some() {
            {
                let mut tasks_w = tasks.write().await;
                if let Some(t) = tasks_w.get_mut(&task.id) {
                    t.status = DownloadStatus::Verifying;
                }
            }
            if let Some(expected_sha256) = &task.sha256 {
                verify_sha256(&task.dest_path, expected_sha256).await?;
            } else if let Some(expected_md5) = &task.md5 {
                verify_md5(&task.dest_path, expected_md5).await?;
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
        _last_bytes: Arc<tokio::sync::Mutex<u64>>,
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
            return Ok(()); // chunk already complete
        }

        let mut request = client.get(&chunk.url);
        if chunk.start != 0 || chunk.end != total_size.saturating_sub(1) {
            request = request.header(
                reqwest::header::RANGE,
                format!("bytes={}-{}", range_start, range_end),
            );
        }

        let response = request.send().await?.error_for_status()?;
        let mut stream = response.bytes_stream();

        // Open file for writing at the right offset
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&dest_path)
            .await?;
        file.seek(SeekFrom::Start(chunk.start + already_downloaded))
            .await?;

        let mut chunk_downloaded = already_downloaded;

        while let Some(item) = stream.next().await {
            let data = item?;
            file.write_all(&data).await?;

            let bytes = data.len() as u64;
            chunk_downloaded += bytes;

            // Update shared counter
            let mut counter = downloaded_counter.lock().await;
            *counter += bytes;
            let total_downloaded = *counter;
            drop(counter);

            // Compute speed (bytes/s)
            let elapsed = start_time.elapsed().as_secs_f64().max(0.001);
            let speed = (total_downloaded as f64 / elapsed) as u64;

            let progress = if total_size > 0 {
                (total_downloaded as f64 / total_size as f64 * 100.0).min(100.0)
            } else {
                0.0
            };

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

        // Mark chunk as completed
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
        if let Some(handle) = self.handles.lock().await.get(task_id) {
            handle.abort();
        }
        let mut tasks = self.tasks.write().await;
        if let Some(t) = tasks.get_mut(task_id) {
            t.status = DownloadStatus::Paused;
        }
        Ok(())
    }

    pub async fn cancel_task(&self, task_id: &str) -> Result<()> {
        if let Some(handle) = self.handles.lock().await.remove(task_id) {
            handle.abort();
        }
        let mut tasks = self.tasks.write().await;
        tasks.remove(task_id);
        Ok(())
    }

    pub async fn get_tasks(&self) -> Vec<DownloadTask> {
        self.tasks.read().await.values().cloned().collect()
    }

    pub async fn get_task(&self, task_id: &str) -> Option<DownloadTask> {
        self.tasks.read().await.get(task_id).cloned()
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
        Err(anyhow!(
            "MD5 mismatch: expected {}, got {}",
            expected,
            actual
        ))
    }
}
