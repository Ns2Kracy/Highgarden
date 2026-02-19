<script lang="ts">
  import { games, selectedGameId, selectedGame, updateGame } from '$lib/stores/games';
  import { downloadTasks, addTask, updateTask, removeTask } from '$lib/stores/downloads';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount, onDestroy } from 'svelte';
  import {
    Play, Download, RefreshCw, FolderOpen,
    AlertCircle, X, Pause, Loader, CheckCircle2,
    MoreHorizontal, Wrench, Trash2
  } from 'lucide-svelte';
  import type { GameId, GameManifest, DownloadProgress, DownloadStatus } from '$lib/types';

  // ─── Per-game download state ───────────────────────────────────────────────
  type DownloadPhase = 'idle' | 'fetching' | 'confirm' | 'downloading' | 'done' | 'launching' | 'playing';

  let phases = $state<Record<GameId, DownloadPhase>>({
    arknights: 'idle',
    endfield: 'idle',
  });
  let manifests = $state<Record<GameId, GameManifest | null>>({
    arknights: null,
    endfield: null,
  });
  let gameTaskIds = $state<Record<GameId, string[]>>({
    arknights: [],
    endfield: [],
  });

  // ─── Toast (error & info) ──────────────────────────────────────────────────
  type ToastKind = 'error' | 'info';
  let toastMsg = $state('');
  let toastKind = $state<ToastKind>('error');
  let toastTimer: ReturnType<typeof setTimeout> | null = null;
  function showError(msg: string) { showToast(msg, 'error'); }
  function showInfo(msg: string)  { showToast(msg, 'info'); }
  function showToast(msg: string, kind: ToastKind) {
    toastMsg = msg; toastKind = kind;
    if (toastTimer) clearTimeout(toastTimer);
    toastTimer = setTimeout(() => { toastMsg = ''; }, 6000);
  }

  // ─── Dropdown menu ─────────────────────────────────────────────────────────
  let menuOpenId = $state<GameId | null>(null);

  // ─── Event listener ────────────────────────────────────────────────────────
  let unlisten: (() => void) | null = null;
  let unlistenStatus: (() => void) | null = null;
  onMount(async () => {
    unlisten = await listen<DownloadProgress>('download:progress', ({ payload }) => {
      updateTask(payload.taskId, {
        downloadedSize: payload.downloadedSize,
        progress: payload.progress,
        speed: payload.speed,
        status: payload.status,
        error: payload.error,
      });
      for (const gameId of ['arknights', 'endfield'] as GameId[]) {
        const ids = gameTaskIds[gameId];
        if (!ids.length) continue;
        const all = $downloadTasks.filter(t => ids.includes(t.id));
        if (all.length && all.every(t => t.status === 'completed')) {
          phases[gameId] = 'done';
        }
      }
    });

    unlistenStatus = await listen<{ gameId: GameId; running: boolean }>('game:status', ({ payload }) => {
      if (payload.running) {
        phases[payload.gameId] = 'playing';
      } else if (phases[payload.gameId] === 'launching' || phases[payload.gameId] === 'playing') {
        phases[payload.gameId] = 'idle';
      }
    });
  });
  onDestroy(() => { unlisten?.(); unlistenStatus?.(); });

  // ─── Helpers ───────────────────────────────────────────────────────────────
  function formatSize(bytes: number): string {
    if (bytes <= 0) return '0 B';
    if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
    return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
  }
  function formatSpeed(bps: number): string { return `${formatSize(bps)}/s`; }

  // ─── Aggregate stats for a game ────────────────────────────────────────────
  function gameStats(gameId: GameId) {
    const ids = gameTaskIds[gameId];
    const tasks = $downloadTasks.filter(t => ids.includes(t.id));
    const downloaded = tasks.reduce((s, t) => s + t.downloadedSize, 0);
    const speed = tasks.filter(t => t.status === 'downloading').reduce((s, t) => s + t.speed, 0);
    const m = manifests[gameId];
    const total = m ? m.packs.reduce((s, p) => s + p.size, 0) : 0;
    const progress = total > 0 ? Math.min((downloaded / total) * 100, 100) : 0;
    const hasError = tasks.some(t => t.status === 'error');
    return { downloaded, speed, progress, hasError, total };
  }

  // ─── Actions ───────────────────────────────────────────────────────────────
  async function selectInstallPath(gameId: GameId) {
    try {
      const result = await invoke<{ path: string; installed: boolean } | null>('select_game_path', { gameId });
      if (result) updateGame(gameId, { installPath: result.path, installed: result.installed });
    } catch (e) {
      showError(`路径设置失败：${e}`);
    }
  }

  async function launchGame(gameId: GameId, installPath: string | null) {
    if (!installPath) { showError('请先设置游戏安装路径'); return; }
    phases[gameId] = 'launching';
    try {
      await invoke('launch_game', { gameId, installPath });
    } catch (e) {
      phases[gameId] = 'idle';
      showError(`启动失败：${e}`);
    }
  }

  async function fetchManifest(gameId: GameId) {
    phases[gameId] = 'fetching';
    try {
      manifests[gameId] = await invoke<GameManifest>('fetch_game_manifest', { gameId });
      phases[gameId] = 'confirm';
    } catch (e) {
      showError(`获取版本信息失败：${e}`);
      phases[gameId] = 'idle';
    }
  }

  async function startInstall(gameId: GameId) {
    const game = $games.find(g => g.id === gameId);
    if (!game?.installPath) { showError('请先设置安装目录'); return; }

    phases[gameId] = 'downloading';
    try {
      const ids = await invoke<string[]>('start_game_install', {
        gameId,
        destDir: game.installPath,
      });
      gameTaskIds[gameId] = ids;
      const m = manifests[gameId]!;
      ids.forEach((id, i) => {
        const pack = m.packs[i];
        addTask({
          id,
          gameId,
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
      phases[gameId] = 'confirm';
    }
  }

  async function pauseAll(gameId: GameId) {
    for (const id of gameTaskIds[gameId]) {
      try { await invoke('pause_download_task', { taskId: id }); } catch {}
    }
  }

  async function cancelInstall(gameId: GameId) {
    for (const id of gameTaskIds[gameId]) {
      try {
        await invoke('cancel_download_task', { taskId: id });
        removeTask(id);
      } catch {}
    }
    gameTaskIds[gameId] = [];
    manifests[gameId] = null;
    phases[gameId] = 'idle';
  }

  async function checkUpdate(gameId: GameId) {
    menuOpenId = null;
    try {
      const version = await invoke<string | null>('fetch_game_version', { gameId });
      if (version) {
        showInfo(`当前最新版本：${version}`);
      } else {
        showInfo('暂时无法获取版本信息');
      }
    } catch (e) {
      showError(`检查更新失败：${e}`);
    }
  }

  async function clearCache(gameId: GameId, installPath: string | null) {
    menuOpenId = null;
    if (!installPath) { showError('请先设置游戏安装路径'); return; }
    try {
      await invoke('clear_game_cache', { gameId, installPath });
      showInfo('缓存已清除');
    } catch (e) {
      showError(`清除缓存失败：${e}`);
    }
  }

  function repairGame(gameId: GameId) {
    menuOpenId = null;
    showInfo('游戏修复功能暂未开放');
  }

  // ─── UI data ───────────────────────────────────────────────────────────────
  const gameColors: Record<GameId, string> = {
    arknights: 'var(--color-ak-blue)',
    endfield: 'var(--color-endfield-cyan)',
  };

  const statusLabel: Record<DownloadStatus, string> = {
    pending: '等待',
    downloading: '下载中',
    paused: '已暂停',
    verifying: '校验中',
    completed: '完成',
    error: '出错',
  };
</script>

<div class="game-library">
  <!-- Game selector tabs -->
  <div class="game-tabs">
    {#each $games as game}
      <button
        class="game-tab"
        class:active={$selectedGameId === game.id}
        onclick={() => selectedGameId.set(game.id)}
        style="--game-color: {gameColors[game.id]}"
      >
        {game.name}
        {#if phases[game.id] === 'downloading'}
          <span class="tab-badge downloading">下载中</span>
        {:else if phases[game.id] === 'launching'}
          <span class="tab-badge launching">启动中</span>
        {:else if phases[game.id] === 'playing'}
          <span class="tab-badge playing">运行中</span>
        {:else if game.updateAvailable}
          <span class="tab-badge update">更新</span>
        {/if}
      </button>
    {/each}
  </div>

  {#if toastMsg}
    <div class="toast" class:toast-info={toastKind === 'info'} class:toast-error={toastKind === 'error'}>
      {#if toastKind === 'error'}<AlertCircle size={14} />{:else}<CheckCircle2 size={14} />{/if}
      <span>{toastMsg}</span>
      <button class="toast-close" onclick={() => { toastMsg = ''; }}><X size={12} /></button>
    </div>
  {/if}

  <!-- Game panel -->
  {#if $selectedGame}
    {@const game = $selectedGame}
    {@const phase = phases[game.id]}
    {@const manifest = manifests[game.id]}
    {@const stats = gameStats(game.id)}

    <div class="game-panel" style="--game-color: {gameColors[game.id]}">
      <div class="game-bg"><div class="game-bg-gradient"></div></div>

      <div class="game-content">
        <div class="game-info">
          <div class="game-label">HYPERGRYPH</div>
          <h1 class="game-name">{game.name}</h1>
          <p class="game-name-en">{game.nameEn}</p>

          {#if game.version}
            <div class="version-badge">
              v{game.version}
              {#if game.updateAvailable}
                <span class="update-pill">→ v{game.latestVersion}</span>
              {/if}
            </div>
          {/if}

          {#if game.installPath}
            <div class="install-path">
              <FolderOpen size={12} />
              <span>{game.installPath}</span>
            </div>
          {/if}
        </div>

        <!-- ── Actions area ── -->
        <div class="actions-area">

          <!-- IDLE / not installed -->
          {#if phase === 'idle' && !game.installed}
            <div class="action-row">
              <button class="btn btn-primary" onclick={() => fetchManifest(game.id)}>
                <Download size={16} />
                <span>下载游戏</span>
              </button>
              <button class="btn btn-secondary" onclick={() => selectInstallPath(game.id)}>
                <FolderOpen size={16} />
                <span>已有安装</span>
              </button>
            </div>

          <!-- IDLE / installed -->
          {:else if phase === 'idle' && game.installed}
            <div class="action-row">
              {#if game.updateAvailable}
                <button class="btn btn-primary" onclick={() => fetchManifest(game.id)}>
                  <RefreshCw size={16} />
                  <span>更新游戏</span>
                </button>
              {:else}
                <button class="btn btn-primary" onclick={() => launchGame(game.id, game.installPath)}>
                  <Play size={16} />
                  <span>开始游戏</span>
                </button>
              {/if}

              <!-- More-actions dropdown trigger -->
              <div class="more-menu">
                <button
                  class="btn-icon"
                  title="更多操作"
                  onclick={() => { menuOpenId = menuOpenId === game.id ? null : game.id; }}
                >
                  <MoreHorizontal size={18} />
                </button>

                {#if menuOpenId === game.id}
                  <div class="dropdown">
                    <button class="dd-item" onclick={() => selectInstallPath(game.id)}>
                      <FolderOpen size={14} />
                      <span>更改路径</span>
                    </button>
                    <button class="dd-item" onclick={() => checkUpdate(game.id)}>
                      <RefreshCw size={14} />
                      <span>检查更新</span>
                    </button>
                    <button class="dd-item" onclick={() => repairGame(game.id)}>
                      <Wrench size={14} />
                      <span>游戏修复</span>
                    </button>
                    <div class="dd-divider"></div>
                    <button class="dd-item dd-danger" onclick={() => clearCache(game.id, game.installPath)}>
                      <Trash2 size={14} />
                      <span>清除缓存</span>
                    </button>
                  </div>
                {/if}
              </div>
            </div>

            <!-- Backdrop to close dropdown on outside click -->
            {#if menuOpenId === game.id}
              <div class="menu-backdrop" onclick={() => { menuOpenId = null; }}></div>
            {/if}

          <!-- LAUNCHING -->
          {:else if phase === 'launching'}
            <div class="action-row">
              <button class="btn btn-primary" disabled>
                <Loader size={16} class="spin" />
                <span>启动中…</span>
              </button>
            </div>

          <!-- PLAYING -->
          {:else if phase === 'playing'}
            <div class="action-row">
              <button class="btn btn-playing" disabled>
                <span class="playing-dot"></span>
                <span>正在游戏</span>
              </button>
            </div>

          <!-- FETCHING -->
          {:else if phase === 'fetching'}
            <div class="status-row">
              <Loader size={16} class="spin" />
              <span>正在获取版本信息…</span>
            </div>

          <!-- CONFIRM -->
          {:else if phase === 'confirm' && manifest}
            <div class="confirm-panel">
              <div class="confirm-info">
                <div class="ci-row">
                  <span class="ci-key">版本</span>
                  <span class="ci-val">{manifest.version}</span>
                </div>
                <div class="ci-row">
                  <span class="ci-key">分包数</span>
                  <span class="ci-val">{manifest.packs.length} 个</span>
                </div>
                <div class="ci-row">
                  <span class="ci-key">下载大小</span>
                  <span class="ci-val accent">{formatSize(manifest.packs.reduce((s, p) => s + p.size, 0))}</span>
                </div>
                <div class="ci-row">
                  <span class="ci-key">安装大小</span>
                  <span class="ci-val">{formatSize(manifest.totalSize)}</span>
                </div>
                {#if !game.installPath}
                  <div class="ci-row warn">
                    <AlertCircle size={13} />
                    <span>请先选择安装目录</span>
                    <button class="btn-link" onclick={() => selectInstallPath(game.id)}>选择</button>
                  </div>
                {/if}
              </div>
              <div class="action-row">
                <button
                  class="btn btn-primary"
                  onclick={() => startInstall(game.id)}
                  disabled={!game.installPath}
                >
                  <Download size={16} />
                  <span>开始下载</span>
                </button>
                <button class="btn btn-secondary" onclick={() => { phases[game.id] = 'idle'; manifests[game.id] = null; }}>
                  取消
                </button>
              </div>
            </div>

          <!-- DOWNLOADING -->
          {:else if phase === 'downloading'}
            <div class="download-panel">
              <div class="dl-header">
                <span class="dl-version">v{manifest?.version}</span>
                <span class="dl-meta">
                  {formatSize(stats.downloaded)} / {formatSize(stats.total)}
                  {#if stats.speed > 0}
                    <span class="speed-chip">{formatSpeed(stats.speed)}</span>
                  {/if}
                </span>
              </div>
              <div class="progress-bar">
                <div class="progress-fill" style="width: {stats.progress}%"></div>
              </div>
              <div class="dl-footer">
                <span class="dl-pct">{stats.progress.toFixed(1)}%</span>
                {#if stats.hasError}
                  <span class="dl-error"><AlertCircle size={12} /> 部分分包出错</span>
                {/if}
                <div class="dl-controls">
                  <button class="ctrl-btn" title="暂停全部" onclick={() => pauseAll(game.id)}>
                    <Pause size={13} />
                  </button>
                  <button class="ctrl-btn danger" title="取消" onclick={() => cancelInstall(game.id)}>
                    <X size={13} />
                  </button>
                </div>
              </div>
            </div>

          <!-- DONE -->
          {:else if phase === 'done'}
            <div class="action-row">
              <div class="done-badge">
                <CheckCircle2 size={16} />
                <span>下载完成</span>
              </div>
              <button class="btn btn-primary" onclick={() => launchGame(game.id, game.installPath)}>
                <Play size={16} />
                <span>开始游戏</span>
              </button>
            </div>
          {/if}

        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .game-library {
    height: 100%;
    display: flex;
    flex-direction: column;
  }

  /* Tabs */
  .game-tabs {
    display: flex;
    border-bottom: 1px solid var(--color-border);
    padding: 0 24px;
    background: var(--color-bg-secondary);
    flex-shrink: 0;
  }

  .game-tab {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 20px;
    height: 44px;
    border: none;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 13px;
    font-weight: 500;
    border-bottom: 2px solid transparent;
    transition: all 0.15s ease;
    font-family: var(--font-ui);
    margin-bottom: -1px;
  }
  .game-tab:hover { color: var(--color-text-primary); }
  .game-tab.active {
    color: var(--color-text-primary);
    border-bottom-color: var(--game-color, var(--color-accent));
  }

  .tab-badge {
    font-size: 10px;
    padding: 1px 6px;
    border-radius: 10px;
    font-weight: 600;
  }
  .tab-badge.update { background: var(--color-warning); color: #000; }
  .tab-badge.downloading { background: var(--color-ak-blue); color: #fff; }
  .tab-badge.launching { background: var(--color-accent-dim); color: var(--color-accent); }
  .tab-badge.playing { background: rgba(34,197,94,0.2); color: #22c55e; }

  /* Error toast */
  .toast {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 16px;
    font-size: 13px;
    flex-shrink: 0;
  }
  .toast-error {
    background: rgba(248,113,113,0.1);
    border-bottom: 1px solid rgba(248,113,113,0.25);
    color: var(--color-error);
  }
  .toast-info {
    background: rgba(59,130,246,0.1);
    border-bottom: 1px solid rgba(59,130,246,0.25);
    color: var(--color-ak-blue);
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

  /* Game panel */
  .game-panel {
    position: relative;
    flex: 1;
    overflow: hidden;
  }

  .game-bg {
    position: absolute;
    inset: 0;
    background: linear-gradient(135deg, #0a0c12 0%, #0d1019 60%, #111827 100%);
  }
  .game-bg-gradient {
    position: absolute;
    inset: 0;
    background: radial-gradient(ellipse at 75% 60%, rgba(59,130,246,0.07) 0%, transparent 55%);
  }

  .game-content {
    position: relative;
    z-index: 1;
    height: 100%;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    padding: 40px 48px;
    gap: 20px;
  }

  /* Game info */
  .game-info { display: flex; flex-direction: column; gap: 4px; }
  .game-label {
    font-size: 11px;
    font-weight: 700;
    color: var(--color-accent);
    letter-spacing: 0.2em;
    margin-bottom: 4px;
  }
  .game-name {
    font-size: 42px;
    font-weight: 700;
    color: var(--color-text-primary);
    line-height: 1.1;
    letter-spacing: -0.02em;
  }
  .game-name-en { font-size: 14px; color: var(--color-text-muted); letter-spacing: 0.05em; }

  .version-badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--color-text-secondary);
    background: var(--color-bg-elevated);
    border: 1px solid var(--color-border);
    padding: 4px 10px;
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    width: fit-content;
    margin-top: 6px;
  }
  .update-pill { color: var(--color-warning); }

  .install-path {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--color-text-muted);
    font-family: var(--font-mono);
    margin-top: 4px;
  }

  /* Action area */
  .actions-area { display: flex; flex-direction: column; gap: 12px; }

  .action-row { display: flex; align-items: center; gap: 12px; }

  .status-row {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 13px;
    color: var(--color-text-secondary);
  }

  /* Confirm panel */
  .confirm-panel {
    display: flex;
    flex-direction: column;
    gap: 12px;
    background: rgba(10,12,18,0.7);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    padding: 16px 20px;
    backdrop-filter: blur(8px);
    width: fit-content;
    min-width: 320px;
  }
  .confirm-info { display: flex; flex-direction: column; gap: 6px; }
  .ci-row {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 13px;
  }
  .ci-key { color: var(--color-text-muted); width: 64px; flex-shrink: 0; }
  .ci-val { color: var(--color-text-primary); }
  .ci-val.accent { color: var(--color-accent); font-weight: 600; }
  .ci-row.warn { color: var(--color-warning); gap: 6px; }
  .btn-link {
    background: none;
    border: none;
    color: var(--color-accent);
    cursor: pointer;
    font-size: 13px;
    font-family: var(--font-ui);
    text-decoration: underline;
    padding: 0;
  }

  /* Download panel */
  .download-panel {
    display: flex;
    flex-direction: column;
    gap: 8px;
    background: rgba(10,12,18,0.7);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    padding: 16px 20px;
    backdrop-filter: blur(8px);
    width: 400px;
  }

  .dl-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 12px;
  }
  .dl-version { color: var(--color-text-secondary); font-family: var(--font-mono); }
  .dl-meta { display: flex; align-items: center; gap: 8px; color: var(--color-text-muted); font-family: var(--font-mono); }
  .speed-chip {
    background: rgba(59,130,246,0.15);
    color: var(--color-ak-blue);
    padding: 1px 7px;
    border-radius: 10px;
    font-size: 11px;
  }

  .progress-bar {
    height: 5px;
    background: var(--color-bg-elevated);
    border-radius: 3px;
    overflow: hidden;
  }
  .progress-fill {
    height: 100%;
    background: var(--color-accent);
    border-radius: 3px;
    transition: width 0.4s ease;
  }

  .dl-footer {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .dl-pct { font-size: 20px; font-weight: 700; color: var(--color-accent); font-family: var(--font-mono); }
  .dl-error { display: flex; align-items: center; gap: 5px; font-size: 12px; color: var(--color-error); }
  .dl-controls { display: flex; gap: 6px; margin-left: auto; }

  .ctrl-btn {
    display: flex; align-items: center; justify-content: center;
    width: 28px; height: 28px;
    border: 1px solid var(--color-border);
    background: var(--color-bg-elevated);
    color: var(--color-text-secondary);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: all 0.1s;
  }
  .ctrl-btn:hover { border-color: var(--color-border-hover); color: var(--color-text-primary); }
  .ctrl-btn.danger:hover { border-color: var(--color-error); color: var(--color-error); }

  /* Done badge */
  .done-badge {
    display: flex;
    align-items: center;
    gap: 7px;
    color: var(--color-success);
    font-size: 14px;
    font-weight: 600;
  }

  /* Buttons */
  .btn {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 24px;
    border-radius: var(--radius-md);
    font-size: 14px;
    font-weight: 600;
    cursor: pointer;
    border: none;
    transition: all 0.15s ease;
    font-family: var(--font-ui);
  }
  .btn:disabled { opacity: 0.45; cursor: not-allowed; }
  .btn-primary { background: var(--color-accent); color: #0d0e14; min-width: 140px; justify-content: center; }
  .btn-primary:hover:not(:disabled) { background: #f0d896; box-shadow: 0 0 20px var(--color-accent-glow); }
  .btn-primary:active:not(:disabled) { transform: scale(0.98); }
  .btn-secondary {
    background: var(--color-bg-elevated);
    color: var(--color-text-secondary);
    border: 1px solid var(--color-border);
  }
  .btn-secondary:hover { border-color: var(--color-border-hover); color: var(--color-text-primary); }

  .btn-playing {
    background: rgba(34, 197, 94, 0.12);
    color: #22c55e;
    border: 1px solid rgba(34, 197, 94, 0.3);
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 24px;
    border-radius: var(--radius-md);
    font-size: 14px;
    font-weight: 600;
    cursor: default;
    font-family: var(--font-ui);
    min-width: 140px;
    justify-content: center;
  }

  .playing-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: #22c55e;
    flex-shrink: 0;
    animation: pulse 2s ease-in-out infinite;
  }

  :global(.spin) { animation: spin 1s linear infinite; }
  @keyframes spin { from { transform: rotate(0deg); } to { transform: rotate(360deg); } }
  @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.35; } }

  /* More-actions icon button */
  .btn-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    border-radius: var(--radius-md);
    border: 1px solid var(--color-border);
    background: var(--color-bg-elevated);
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all 0.15s ease;
    flex-shrink: 0;
  }
  .btn-icon:hover { border-color: var(--color-border-hover); color: var(--color-text-primary); }

  /* Dropdown container */
  .more-menu {
    position: relative;
  }

  .dropdown {
    position: absolute;
    bottom: calc(100% + 8px);
    left: 0;
    min-width: 160px;
    background: var(--color-bg-elevated);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    padding: 4px;
    z-index: 100;
    box-shadow: 0 8px 24px rgba(0,0,0,0.4);
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .dd-item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    border: none;
    background: transparent;
    color: var(--color-text-secondary);
    cursor: pointer;
    font-size: 13px;
    font-family: var(--font-ui);
    border-radius: var(--radius-sm);
    text-align: left;
    transition: background 0.1s ease, color 0.1s ease;
    white-space: nowrap;
  }
  .dd-item:hover { background: var(--color-bg-secondary); color: var(--color-text-primary); }
  .dd-item.dd-danger { color: var(--color-error); }
  .dd-item.dd-danger:hover { background: rgba(248,113,113,0.1); color: var(--color-error); }

  .dd-divider {
    height: 1px;
    background: var(--color-border);
    margin: 4px 0;
  }

  /* Backdrop for closing dropdown */
  .menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 99;
  }
</style>
