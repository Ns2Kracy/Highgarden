<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { settings } from '$lib/stores/settings';
  import { downloadTasks, addTask, updateTask, removeTask } from '$lib/stores/downloads';
  import { Download, Pause, Play, Trash2, FolderOpen, RefreshCw, AlertCircle, X } from 'lucide-svelte';
  import type { GameManifest, DownloadProgress, DownloadStatus, GameId } from '$lib/types';
  import { onMount, onDestroy } from 'svelte';

  type Phase = 'idle' | 'fetching' | 'confirm' | 'downloading' | 'done';

  let phase = $state<Phase>('idle');
  let manifest = $state<GameManifest | null>(null);
  let selectedGame = $state<GameId>('arknights');
  let destDir = $state($settings.downloadPath || '');
  let errorMsg = $state('');
  let taskIds = $state<string[]>([]);

  // Aggregate progress across all packs
  let totalDownloaded = $derived(
    $downloadTasks
      .filter(t => taskIds.includes(t.id))
      .reduce((acc, t) => acc + t.downloadedSize, 0)
  );
  let totalSpeed = $derived(
    $downloadTasks
      .filter(t => taskIds.includes(t.id) && t.status === 'downloading')
      .reduce((acc, t) => acc + t.speed, 0)
  );
  let overallProgress = $derived(
    manifest && manifest.totalSize > 0
      ? Math.min((totalDownloaded / manifest.totalSize) * 100, 100)
      : 0
  );
  // All tasks in the store (including ones loaded from previous sessions)
  let activeTasks = $derived($downloadTasks);
  // Show the setup panel only when there are no incomplete tasks, or when the
  // user has explicitly started the version-check / confirm flow, or manually toggled.
  let hasIncomplete = $derived(
    activeTasks.some((t) => t.status !== 'completed' && t.status !== 'error')
  );
  let forceShowSetup = $state(false);
  let showSetup = $derived(
    forceShowSetup || !hasIncomplete || phase === 'fetching' || phase === 'confirm'
  );

  let unlisten: (() => void) | null = null;

  onMount(async () => {
    // Load persisted tasks
    try {
      const tasks = await invoke<any[]>('get_download_tasks');
      tasks.forEach(t => addTask(t));
    } catch {}

    // Listen to progress events from Rust
    unlisten = await listen<DownloadProgress>('download:progress', ({ payload }) => {
      updateTask(payload.taskId, {
        downloadedSize: payload.downloadedSize,
        progress: payload.progress,
        speed: payload.speed,
        status: payload.status,
        error: payload.error,
      });
    });
  });

  onDestroy(() => {
    unlisten?.();
  });

  function formatSize(bytes: number): string {
    if (bytes <= 0) return '0 B';
    if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
    return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
  }

  function formatSpeed(bps: number): string {
    return `${formatSize(bps)}/s`;
  }

  function showError(msg: string) {
    errorMsg = msg;
    setTimeout(() => { errorMsg = ''; }, 6000);
  }

  async function selectDestDir() {
    try {
      const path = await invoke<string | null>('select_download_path');
      if (path) destDir = path;
    } catch (e) {
      showError(`选择目录失败：${e}`);
    }
  }

  async function fetchManifest() {
    if (!destDir) { showError('请先选择下载目录'); return; }
    phase = 'fetching';
    errorMsg = '';
    try {
      manifest = await invoke<GameManifest>('fetch_game_manifest', { gameId: selectedGame });
      phase = 'confirm';
    } catch (e) {
      showError(`获取版本信息失败：${e}`);
      phase = 'idle';
    }
  }

  async function startInstall() {
    if (!manifest || !destDir) return;
    phase = 'downloading';
    forceShowSetup = false;
    try {
      const ids = await invoke<string[]>('start_game_install', {
        gameId: selectedGame,
        destDir,
      });
      taskIds = ids;
      // Add placeholder tasks to store so UI shows them immediately
      ids.forEach((id, i) => {
        const pack = manifest!.packs[i];
        addTask({
          id,
          gameId: selectedGame,
          name: pack.filename,
          totalSize: pack.size,
          downloadedSize: 0,
          progress: 0,
          speed: 0,
          status: 'downloading',
          error: null,
          createdAt: Date.now(),
        });
      });
    } catch (e) {
      showError(`启动下载失败：${e}`);
      phase = 'confirm';
    }
  }

  async function pauseTask(taskId: string) {
    try {
      await invoke('pause_download_task', { taskId });
      updateTask(taskId, { status: 'paused', speed: 0 });
    } catch (e) { showError(`${e}`); }
  }

  async function resumeTask(taskId: string) {
    try {
      await invoke('start_download_task', { taskId });
      updateTask(taskId, { status: 'downloading' });
    } catch (e) { showError(`${e}`); }
  }

  async function cancelTask(taskId: string) {
    try {
      await invoke('cancel_download_task', { taskId });
      removeTask(taskId);
      taskIds = taskIds.filter(id => id !== taskId);
    } catch (e) { showError(`${e}`); }
  }

  const gameNames: Record<GameId, string> = {
    arknights: '明日方舟',
    endfield: '明日方舟：终末地',
  };

  const statusLabel: Record<DownloadStatus, string> = {
    pending: '等待中',
    downloading: '下载中',
    paused: '已暂停',
    verifying: '校验中',
    completed: '已完成',
    error: '出错',
  };

  const statusColor: Record<DownloadStatus, string> = {
    pending: 'var(--color-text-muted)',
    downloading: 'var(--color-ak-blue)',
    paused: 'var(--color-warning)',
    verifying: 'var(--color-endfield-cyan)',
    completed: 'var(--color-success)',
    error: 'var(--color-error)',
  };
</script>

<div class="download-page">
  <div class="page-header">
    <div class="header-left">
      <h2 class="page-title">下载管理</h2>
      <p class="page-subtitle">游戏安装与更新</p>
    </div>
    {#if !showSetup}
      <button class="btn-new" onclick={() => { forceShowSetup = true; phase = 'idle'; }}>
        <Download size={13} />
        <span>新建下载</span>
      </button>
    {/if}
  </div>

  {#if errorMsg}
    <div class="error-toast">
      <AlertCircle size={14} />
      <span>{errorMsg}</span>
      <button class="toast-close" onclick={() => { errorMsg = ''; }}><X size={12} /></button>
    </div>
  {/if}

  <!-- New download setup panel -->
  {#if showSetup}
    <div class="setup-panel">
      <div class="setup-section">
        <label class="setup-label">游戏</label>
        <div class="game-selector">
          {#each Object.entries(gameNames) as [id, name]}
            <button
              class="game-chip"
              class:active={selectedGame === id}
              onclick={() => { selectedGame = id as GameId; manifest = null; phase = 'idle'; }}
            >{name}</button>
          {/each}
        </div>
      </div>

      <div class="setup-section">
        <label class="setup-label">安装目录</label>
        <div class="path-row">
          <span class="path-text">{destDir || '未选择'}</span>
          <button class="btn-sm" onclick={selectDestDir}>
            <FolderOpen size={13} />
            <span>选择</span>
          </button>
        </div>
      </div>

      {#if phase === 'confirm' && manifest}
        <div class="manifest-info">
          <div class="manifest-row">
            <span class="mkey">版本</span>
            <span class="mval">{manifest.version}</span>
          </div>
          <div class="manifest-row">
            <span class="mkey">分包数</span>
            <span class="mval">{manifest.packs.length} 个</span>
          </div>
          <div class="manifest-row">
            <span class="mkey">总大小</span>
            <span class="mval accent">{formatSize(manifest.totalSize)}</span>
          </div>
          <div class="manifest-row">
            <span class="mkey">校验值</span>
            <span class="mval mono">{manifest.gameFilesMd5.slice(0, 16)}…</span>
          </div>
        </div>
        <div class="setup-actions">
          <button class="btn btn-primary" onclick={startInstall}>
            <Download size={15} />
            <span>开始下载</span>
          </button>
          <button class="btn btn-ghost" onclick={() => { phase = 'idle'; manifest = null; forceShowSetup = false; }}>取消</button>
        </div>
      {:else}
        <div class="setup-actions">
          <button class="btn btn-primary" onclick={fetchManifest} disabled={phase === 'fetching'}>
            {#if phase === 'fetching'}
              <RefreshCw size={15} class="spin" />
              <span>获取中…</span>
            {:else}
              <Download size={15} />
              <span>检查版本</span>
            {/if}
          </button>
        </div>
      {/if}
    </div>
  {/if}

  <!-- Active downloads -->
  {#if activeTasks.length > 0}
    <div class="tasks-section">
      <!-- Overall progress bar -->
      {#if manifest && taskIds.length > 0}
        <div class="overall-progress">
          <div class="op-header">
            <span class="op-label">{gameNames[selectedGame]} — {manifest.version}</span>
            <span class="op-meta">
              {formatSize(totalDownloaded)} / {formatSize(manifest.totalSize)}
              {#if totalSpeed > 0}
                <span class="speed-pill">{formatSpeed(totalSpeed)}</span>
              {/if}
            </span>
          </div>
          <div class="progress-bar large">
            <div class="progress-fill" style="width: {overallProgress}%"></div>
          </div>
          <div class="op-pct">{overallProgress.toFixed(1)}%</div>
        </div>
      {/if}

      <!-- Pack list -->
      <div class="pack-list">
        {#each activeTasks as task (task.id)}
          <div class="task-card" class:paused={task.status === 'paused'}>
            <div class="task-header">
              <span class="task-name">{task.name}</span>
              <div class="task-right">
                <span class="task-status" style="color: {statusColor[task.status]}">
                  {statusLabel[task.status]}
                </span>
                <div class="task-controls">
                  {#if task.status === 'downloading'}
                    <button class="ctrl-btn" title="暂停" onclick={() => pauseTask(task.id)}>
                      <Pause size={12} />
                    </button>
                  {:else if task.status === 'paused'}
                    <button class="ctrl-btn resume" title="继续" onclick={() => resumeTask(task.id)}>
                      <Play size={12} />
                    </button>
                  {/if}
                  {#if task.status !== 'completed' && task.status !== 'verifying'}
                    <button class="ctrl-btn danger" title="取消" onclick={() => cancelTask(task.id)}>
                      <Trash2 size={12} />
                    </button>
                  {/if}
                </div>
              </div>
            </div>

            <div class="progress-bar">
              <div
                class="progress-fill"
                class:paused={task.status === 'paused'}
                class:verifying={task.status === 'verifying'}
                style="width: {task.progress}%"
              ></div>
            </div>

            <div class="task-meta">
              <span>{formatSize(task.downloadedSize)} / {formatSize(task.totalSize)}</span>
              {#if task.status === 'downloading' && task.speed > 0}
                <span class="speed">{formatSpeed(task.speed)}</span>
              {/if}
              {#if task.status === 'paused'}
                <span class="paused-label">已暂停</span>
              {/if}
              {#if task.error}
                <span class="err">{task.error}</span>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    </div>
  {:else if phase === 'idle' && activeTasks.length === 0}
    <div class="empty-state">
      <Download size={40} strokeWidth={1} />
      <p>暂无下载任务</p>
      <span>在上方选择游戏并开始下载</span>
    </div>
  {/if}
</div>

<style>
  .download-page {
    height: 100%;
    display: flex;
    flex-direction: column;
    padding: 28px 32px;
    gap: 20px;
    overflow-y: auto;
  }

  .page-header { flex-shrink: 0; display: flex; align-items: flex-end; justify-content: space-between; }
  .page-title { font-size: 22px; font-weight: 700; color: var(--color-text-primary); }
  .page-subtitle { font-size: 13px; color: var(--color-text-muted); margin-top: 2px; }

  .btn-new {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 6px 14px;
    background: var(--color-bg-elevated);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    color: var(--color-text-secondary);
    font-size: 12px;
    font-family: var(--font-ui);
    cursor: pointer;
    white-space: nowrap;
    transition: all 0.1s;
    flex-shrink: 0;
  }
  .btn-new:hover { border-color: var(--color-border-hover); color: var(--color-text-primary); }

  /* Error toast */
  .error-toast {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    background: rgba(248,113,113,0.1);
    border: 1px solid rgba(248,113,113,0.25);
    border-radius: var(--radius-md);
    color: var(--color-error);
    font-size: 13px;
    flex-shrink: 0;
  }
  .toast-close {
    margin-left: auto;
    background: none;
    border: none;
    color: inherit;
    cursor: pointer;
    opacity: 0.7;
    display: flex;
    align-items: center;
  }
  .toast-close:hover { opacity: 1; }

  /* Setup panel */
  .setup-panel {
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    padding: 20px 24px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    flex-shrink: 0;
  }

  .setup-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .setup-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--color-text-muted);
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .game-selector {
    display: flex;
    gap: 8px;
  }

  .game-chip {
    padding: 6px 16px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--color-border);
    background: var(--color-bg-elevated);
    color: var(--color-text-secondary);
    font-size: 13px;
    font-family: var(--font-ui);
    cursor: pointer;
    transition: all 0.1s;
  }
  .game-chip:hover { border-color: var(--color-border-hover); color: var(--color-text-primary); }
  .game-chip.active {
    border-color: var(--color-accent-dim);
    color: var(--color-accent);
    background: var(--color-accent-glow);
  }

  .path-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .path-text {
    font-size: 12px;
    color: var(--color-text-muted);
    font-family: var(--font-mono);
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .btn-sm {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 5px 12px;
    background: var(--color-bg-elevated);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    color: var(--color-text-secondary);
    font-size: 12px;
    font-family: var(--font-ui);
    cursor: pointer;
    white-space: nowrap;
    transition: all 0.1s;
    flex-shrink: 0;
  }
  .btn-sm:hover { border-color: var(--color-border-hover); color: var(--color-text-primary); }

  /* Manifest info */
  .manifest-info {
    background: var(--color-bg-elevated);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    padding: 12px 16px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .manifest-row {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 13px;
  }
  .mkey { color: var(--color-text-muted); width: 60px; flex-shrink: 0; }
  .mval { color: var(--color-text-primary); }
  .mval.accent { color: var(--color-accent); font-weight: 600; }
  .mval.mono { font-family: var(--font-mono); font-size: 12px; }

  .setup-actions {
    display: flex;
    gap: 10px;
    align-items: center;
  }

  .btn {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 9px 20px;
    border-radius: var(--radius-md);
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    border: none;
    font-family: var(--font-ui);
    transition: all 0.15s;
  }
  .btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .btn-primary { background: var(--color-accent); color: #0d0e14; }
  .btn-primary:hover:not(:disabled) { background: #f0d896; box-shadow: 0 0 16px var(--color-accent-glow); }
  .btn-ghost { background: transparent; color: var(--color-text-muted); }
  .btn-ghost:hover { color: var(--color-text-primary); }

  /* Overall progress */
  .tasks-section {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .overall-progress {
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    padding: 16px 20px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .op-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .op-label { font-size: 14px; font-weight: 600; color: var(--color-text-primary); }
  .op-meta { display: flex; align-items: center; gap: 10px; font-size: 12px; color: var(--color-text-muted); font-family: var(--font-mono); }
  .speed-pill {
    background: rgba(59,130,246,0.15);
    color: var(--color-ak-blue);
    padding: 2px 8px;
    border-radius: 10px;
    font-size: 11px;
  }
  .op-pct { font-size: 22px; font-weight: 700; color: var(--color-accent); font-family: var(--font-mono); }

  /* Progress bar */
  .progress-bar {
    height: 4px;
    background: var(--color-bg-elevated);
    border-radius: 2px;
    overflow: hidden;
  }
  .progress-bar.large { height: 6px; }
  .progress-fill {
    height: 100%;
    background: var(--color-accent);
    border-radius: 2px;
    transition: width 0.3s ease;
  }
  .progress-fill.paused {
    background: var(--color-warning, #eab308);
    animation: none;
  }
  .progress-fill.verifying {
    background: var(--color-endfield-cyan);
    animation: pulse 1s ease-in-out infinite;
  }
  @keyframes pulse { 0%,100% { opacity:1; } 50% { opacity:0.6; } }

  /* Pack list */
  .pack-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-height: 340px;
    overflow-y: auto;
  }

  .task-card {
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    transition: border-color 0.15s, opacity 0.15s;
  }
  .task-card:hover { border-color: var(--color-border-hover); }
  .task-card.paused { opacity: 0.7; border-color: rgba(234,179,8,0.25); }

  .task-header { display: flex; justify-content: space-between; align-items: center; gap: 12px; }
  .task-name { font-size: 12px; color: var(--color-text-secondary); font-family: var(--font-mono); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; }
  .task-right { display: flex; align-items: center; gap: 8px; flex-shrink: 0; }
  .task-status { font-size: 11px; font-weight: 500; }

  .task-controls { display: flex; gap: 3px; }
  .ctrl-btn {
    display: flex; align-items: center; justify-content: center;
    width: 24px; height: 24px;
    border: 1px solid var(--color-border);
    background: var(--color-bg-elevated);
    color: var(--color-text-secondary);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: all 0.1s;
  }
  .ctrl-btn:hover { border-color: var(--color-border-hover); color: var(--color-text-primary); }
  .ctrl-btn.resume { border-color: rgba(234,179,8,0.4); color: var(--color-warning, #eab308); }
  .ctrl-btn.resume:hover { border-color: var(--color-warning, #eab308); background: rgba(234,179,8,0.1); }
  .ctrl-btn.danger:hover { border-color: var(--color-error); color: var(--color-error); }

  .task-meta {
    display: flex;
    gap: 14px;
    font-size: 11px;
    color: var(--color-text-muted);
    font-family: var(--font-mono);
  }
  .speed { color: var(--color-ak-blue); margin-left: auto; }
  .paused-label { color: var(--color-warning, #eab308); margin-left: auto; font-style: italic; }
  .err { color: var(--color-error); flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  /* Empty state */
  .empty-state {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: var(--color-text-muted);
    padding: 40px 0;
  }
  .empty-state p { font-size: 15px; font-weight: 500; color: var(--color-text-secondary); }
  .empty-state span { font-size: 13px; }

  /* Spin animation for loading */
  :global(.spin) { animation: spin 1s linear infinite; }
  @keyframes spin { from { transform: rotate(0deg); } to { transform: rotate(360deg); } }
</style>
