<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { get } from 'svelte/store';
  import { games } from '$lib/stores/games';
  import { FileDown, Search, Link, RefreshCw, ChevronLeft, ChevronRight, Sparkles, AlertCircle, CheckCircle2 } from 'lucide-svelte';
  import type { GameId } from '$lib/types';

  // ─── Local types matching backend ────────────────────────────────────────────

  interface GachaRecord {
    id: string;
    uid: string;
    gameId: string;
    poolName: string;
    poolType: 'standard' | 'limited' | 'beginner' | 'special';
    itemName: string;
    itemType: 'character' | 'weapon';
    rarity: number;
    timestamp: number;
    isNew: boolean;
    pity: number;
  }

  interface PoolStats {
    poolType: string;
    totalPulls: number;
    sixStarCount: number;
    fiveStarCount: number;
    fourStarCount: number;
    threeStarCount: number;
    sixStarRate: number;
    currentPity: number;
    avgPity: number;
  }

  interface GachaStats {
    uid: string;
    totalPulls: number;
    byPool: Record<string, PoolStats>;
    fetchedAt: number;
  }

  interface GachaData {
    uid: string;
    gameId: string;
    records: GachaRecord[];
    fetchedAt: number;
  }

  // ─── State ───────────────────────────────────────────────────────────────────

  let selectedGame = $state<GameId>('arknights');
  let phase = $state<'idle' | 'scanning' | 'fetching' | 'done' | 'error'>('idle');
  let manualUrl = $state('');
  let showDataPanel = $state(true);
  let statusMsg = $state('');
  let statusKind = $state<'info' | 'ok' | 'error'>('info');

  let gachaData = $state<GachaData | null>(null);
  let stats = $state<GachaStats | null>(null);

  let selectedPool = $state<'all' | 'standard' | 'limited' | 'beginner' | 'special'>('all');
  let currentPage = $state(1);
  let exportMenuOpen = $state(false);

  const PAGE_SIZE = 50;

  const POOL_TABS = [
    { key: 'all', label: '全部' },
    { key: 'limited', label: '限定' },
    { key: 'standard', label: '标准' },
    { key: 'special', label: '特殊' },
    { key: 'beginner', label: '新手' },
  ] as const;

  // ─── Derived ─────────────────────────────────────────────────────────────────

  let allRecords = $derived(gachaData?.records ?? []);

  let filteredRecords = $derived(
    selectedPool === 'all' ? allRecords : allRecords.filter((r) => r.poolType === selectedPool)
  );

  let totalPages = $derived(Math.ceil(filteredRecords.length / PAGE_SIZE));

  let pagedRecords = $derived(
    filteredRecords.slice((currentPage - 1) * PAGE_SIZE, currentPage * PAGE_SIZE)
  );

  let hasRecords = $derived(allRecords.length > 0);

  let currentPoolStats = $derived(
    selectedPool === 'all' || !stats
      ? null
      : (stats.byPool[selectedPool] ?? null)
  );

  let overallSixStarRate = $derived(
    stats && stats.totalPulls > 0
      ? Object.values(stats.byPool)
          .reduce((s, p) => s + p.sixStarCount, 0) /
          stats.totalPulls *
          100
      : 0
  );

  let overallCurrentPity = $derived(
    stats
      ? Math.max(...Object.values(stats.byPool).map((p) => p.currentPity), 0)
      : 0
  );

  let overallAvgPity = $derived(() => {
    if (!stats) return 0;
    const pools = Object.values(stats.byPool).filter((p) => p.sixStarCount > 0);
    if (pools.length === 0) return 0;
    return +(pools.reduce((s, p) => s + p.avgPity, 0) / pools.length).toFixed(1);
  });

  // ─── Lifecycle ───────────────────────────────────────────────────────────────

  onMount(async () => {
    await loadLocalData();
  });

  // Reset page when pool changes
  $effect(() => {
    // eslint-disable-next-line @typescript-eslint/no-unused-expressions
    selectedPool;
    currentPage = 1;
  });

  // Reset data panel when switching games
  $effect(() => {
    // eslint-disable-next-line @typescript-eslint/no-unused-expressions
    selectedGame;
    gachaData = null;
    stats = null;
    manualUrl = '';
    statusMsg = '';
    phase = 'idle';
    showDataPanel = true;
    loadLocalData();
  });

  // ─── Actions ─────────────────────────────────────────────────────────────────

  async function loadLocalData() {
    try {
      const data = await invoke<GachaData | null>('get_local_gacha_records', {
        gameId: selectedGame,
      });
      if (data) {
        gachaData = data;
        const s = await invoke<GachaStats | null>('get_gacha_stats', { gameId: selectedGame });
        stats = s;
        if (data.records.length > 0) {
          showDataPanel = false;
        }
      }
    } catch {
      // No local data yet — keep showDataPanel = true
    }
  }

  async function scanUrl() {
    const gameList = get(games);
    const game = gameList.find((g) => g.id === selectedGame);
    if (!game?.installPath) {
      setStatus('请先在游戏库页面设置游戏安装路径', 'error');
      return;
    }
    phase = 'scanning';
    setStatus('正在扫描游戏缓存文件…', 'info');
    try {
      const url = await invoke<string | null>('scan_gacha_url', {
        gameId: selectedGame,
        installPath: game.installPath,
      });
      if (url) {
        manualUrl = url;
        setStatus('已找到寻访记录链接，请点击「获取记录」', 'ok');
      } else {
        setStatus('未能自动找到链接。请先在游戏内打开寻访记录，再重试；或手动粘贴链接', 'error');
      }
    } catch (e) {
      setStatus(`扫描失败：${e}`, 'error');
    } finally {
      phase = 'idle';
    }
  }

  async function fetchRecords() {
    if (!manualUrl.trim()) return;
    phase = 'fetching';
    setStatus('正在从服务器获取记录（可能需要几十秒）…', 'info');
    try {
      const result = await invoke<{ uid: string; total: number }>('fetch_gacha_records', {
        gameId: selectedGame,
        url: manualUrl.trim(),
      });
      setStatus(`获取完成：共 ${result.total} 条记录（UID ${result.uid}）`, 'ok');
      await loadLocalData();
      phase = 'done';
      if (result.total > 0) showDataPanel = false;
    } catch (e) {
      setStatus(`获取失败：${e}`, 'error');
      phase = 'error';
    }
  }

  async function doExport(format: string) {
    exportMenuOpen = false;
    try {
      const path = await invoke<string | null>('select_gacha_export_path', { format });
      if (!path) return;
      await invoke('export_gacha_records', {
        gameId: selectedGame,
        format,
        destPath: path,
      });
      setStatus(`已导出为 ${path.split('\\').pop() ?? path}`, 'ok');
    } catch (e) {
      setStatus(`导出失败：${e}`, 'error');
    }
  }

  function setStatus(msg: string, kind: 'info' | 'ok' | 'error') {
    statusMsg = msg;
    statusKind = kind;
  }

  function rarityStars(n: number) {
    return '★'.repeat(Math.min(n, 6));
  }

  function formatDate(ts: number) {
    return new Date(ts * 1000).toLocaleString('zh-CN', {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
    });
  }

  function poolLabel(key: string) {
    const map: Record<string, string> = {
      all: '全部',
      standard: '标准',
      limited: '限定',
      beginner: '新手',
      special: '特殊',
    };
    return map[key] ?? key;
  }

  function activeStats(pool: string): PoolStats | null {
    if (!stats) return null;
    if (pool === 'all') return null;
    return stats.byPool[pool] ?? null;
  }
</script>

<div class="gacha-page">
  <!-- ── Header ── -->
  <div class="page-header">
    <div class="header-left">
      <h2 class="page-title">寻访分析</h2>
      <p class="page-subtitle">抽卡记录统计与分析</p>
    </div>

    <div class="header-right">
      <!-- Game selector -->
      <div class="game-tabs">
        <button
          class="game-tab"
          class:active={selectedGame === 'arknights'}
          onclick={() => (selectedGame = 'arknights')}
        >
          明日方舟
        </button>
        <button
          class="game-tab"
          class:active={selectedGame === 'endfield'}
          onclick={() => (selectedGame = 'endfield')}
        >
          终末地
        </button>
      </div>

      <!-- Refresh / re-fetch -->
      {#if hasRecords}
        <button class="btn-icon" title="重新获取记录" onclick={() => (showDataPanel = !showDataPanel)}>
          <RefreshCw size={15} />
        </button>
      {/if}

      <!-- Export -->
      <div class="export-wrap">
        <button
          class="btn-export"
          class:disabled={!hasRecords}
          disabled={!hasRecords}
          onclick={() => (exportMenuOpen = !exportMenuOpen)}
        >
          <FileDown size={15} />
          <span>导出记录</span>
        </button>
        {#if exportMenuOpen}
          <div class="export-menu">
            <button onclick={() => doExport('xlsx')}>Excel (.xlsx)</button>
            <button onclick={() => doExport('csv')}>CSV (.csv)</button>
            <button onclick={() => doExport('json')}>JSON (.json)</button>
          </div>
        {/if}
      </div>
    </div>
  </div>

  <!-- ── Data source panel ── -->
  {#if showDataPanel}
    <div class="data-panel">
      <div class="data-panel-title">
        <Sparkles size={15} />
        <span>获取寻访记录</span>
      </div>
      <p class="data-panel-hint">
        请先在游戏内打开「寻访记录」页面（保持开着），再点击自动扫描；或直接粘贴游戏内置浏览器地址栏的链接。
      </p>

      <div class="scan-row">
        <button
          class="btn-scan"
          class:loading={phase === 'scanning'}
          disabled={phase === 'scanning' || phase === 'fetching'}
          onclick={scanUrl}
        >
          <Search size={14} />
          <span>{phase === 'scanning' ? '扫描中…' : '自动扫描'}</span>
        </button>
        <span class="scan-sep">或</span>
        <div class="url-input-wrap">
          <Link size={13} class="url-icon" />
          <input
            class="url-input"
            type="text"
            placeholder="粘贴寻访记录链接…"
            bind:value={manualUrl}
            onkeydown={(e) => e.key === 'Enter' && fetchRecords()}
          />
        </div>
        <button
          class="btn-fetch"
          disabled={!manualUrl.trim() || phase === 'fetching' || phase === 'scanning'}
          onclick={fetchRecords}
        >
          {phase === 'fetching' ? '获取中…' : '获取记录'}
        </button>
      </div>

      {#if statusMsg}
        <div class="status-msg" class:ok={statusKind === 'ok'} class:error={statusKind === 'error'}>
          {#if statusKind === 'ok'}
            <CheckCircle2 size={13} />
          {:else if statusKind === 'error'}
            <AlertCircle size={13} />
          {/if}
          <span>{statusMsg}</span>
        </div>
      {/if}
    </div>
  {/if}

  {#if hasRecords}
    <!-- ── Overview stats ── -->
    <div class="stats-grid">
      <div class="stat-card">
        <div class="stat-label">总计</div>
        <div class="stat-value">{stats?.totalPulls.toLocaleString() ?? 0}</div>
        <div class="stat-sub">次寻访</div>
      </div>
      <div class="stat-card accent">
        <div class="stat-label">六星出货率</div>
        <div class="stat-value">
          {currentPoolStats
            ? currentPoolStats.sixStarRate.toFixed(1)
            : overallSixStarRate.toFixed(1)}%
        </div>
        <div class="stat-sub">
          {currentPoolStats
            ? `${currentPoolStats.sixStarCount} 个六星`
            : `${Object.values(stats?.byPool ?? {}).reduce((s, p) => s + p.sixStarCount, 0)} 个六星`}
        </div>
      </div>
      <div class="stat-card">
        <div class="stat-label">当前水位</div>
        <div
          class="stat-value"
          class:pity-warn={(currentPoolStats?.currentPity ?? overallCurrentPity) >= 60}
          class:pity-danger={(currentPoolStats?.currentPity ?? overallCurrentPity) >= 80}
        >
          {currentPoolStats?.currentPity ?? overallCurrentPity}
        </div>
        <div class="stat-sub">抽（自上次六星）</div>
      </div>
      <div class="stat-card">
        <div class="stat-label">平均出金抽数</div>
        <div class="stat-value">
          {currentPoolStats ? currentPoolStats.avgPity.toFixed(1) : overallAvgPity().toFixed(1)}
        </div>
        <div class="stat-sub">抽 / 六星</div>
      </div>
    </div>

    <!-- ── Pool sub-stats ── -->
    {#if selectedPool !== 'all' && currentPoolStats}
      <div class="pool-detail">
        <span class="pd-item"><span class="pd-key">总抽数</span>{currentPoolStats.totalPulls}</span>
        <span class="pd-sep">·</span>
        <span class="pd-item"><span class="pd-key">六星</span>{currentPoolStats.sixStarCount}</span>
        <span class="pd-sep">·</span>
        <span class="pd-item"><span class="pd-key">五星</span>{currentPoolStats.fiveStarCount}</span>
        <span class="pd-sep">·</span>
        <span class="pd-item"><span class="pd-key">四星</span>{currentPoolStats.fourStarCount}</span>
        <span class="pd-sep">·</span>
        <span class="pd-item"><span class="pd-key">三星</span>{currentPoolStats.threeStarCount}</span>
      </div>
    {/if}

    <!-- ── Pool tabs ── -->
    <div class="pool-tabs">
      {#each POOL_TABS as tab (tab.key)}
        {@const count =
          tab.key === 'all'
            ? allRecords.length
            : allRecords.filter((r) => r.poolType === tab.key).length}
        <button
          class="pool-tab"
          class:active={selectedPool === tab.key}
          onclick={() => (selectedPool = tab.key)}
        >
          {tab.label}
          {#if count > 0}
            <span class="tab-count">{count}</span>
          {/if}
        </button>
      {/each}
    </div>

    <!-- ── Record table ── -->
    <div class="records-wrap">
      {#if pagedRecords.length === 0}
        <div class="empty-pool">该卡池暂无记录</div>
      {:else}
        <table class="records-table">
          <thead>
            <tr>
              <th>时间</th>
              <th>卡池</th>
              <th>干员 / 物品</th>
              <th>稀有度</th>
              <th title="自上次六星的抽数">水位</th>
              <th>新</th>
            </tr>
          </thead>
          <tbody>
            {#each pagedRecords as record (record.id)}
              <tr class="r{record.rarity}">
                <td class="td-time">{formatDate(record.timestamp)}</td>
                <td class="td-pool">{record.poolName}</td>
                <td class="td-name">{record.itemName}</td>
                <td class="td-rarity">{rarityStars(record.rarity)}</td>
                <td class="td-pity" class:high-pity={record.pity >= 60}>{record.pity}</td>
                <td class="td-new">{record.isNew ? '✓' : ''}</td>
              </tr>
            {/each}
          </tbody>
        </table>

        <!-- Pagination -->
        {#if totalPages > 1}
          <div class="pagination">
            <button
              class="pg-btn"
              disabled={currentPage === 1}
              onclick={() => (currentPage -= 1)}
            >
              <ChevronLeft size={14} />
            </button>
            <span class="pg-info">{currentPage} / {totalPages}</span>
            <button
              class="pg-btn"
              disabled={currentPage === totalPages}
              onclick={() => (currentPage += 1)}
            >
              <ChevronRight size={14} />
            </button>
          </div>
        {/if}
      {/if}
    </div>

    {#if stats?.uid}
      <div class="footer-info">
        UID {stats.uid} · 最后更新 {formatDate(stats.fetchedAt)}
      </div>
    {/if}
  {:else if !showDataPanel}
    <div class="empty-state">
      <Sparkles size={40} strokeWidth={1} />
      <p>暂无记录，请点击右上角刷新图标获取</p>
    </div>
  {/if}
</div>

<!-- Close export menu on outside click -->
{#if exportMenuOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="overlay" onclick={() => (exportMenuOpen = false)}></div>
{/if}

<style>
  /* ─── Layout ─────────────────────────────────────────────────────────────── */
  .gacha-page {
    height: 100%;
    display: flex;
    flex-direction: column;
    padding: 28px 32px 16px;
    gap: 16px;
    overflow-y: auto;
  }

  /* ─── Header ─────────────────────────────────────────────────────────────── */
  .page-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-shrink: 0;
  }

  .page-title {
    font-size: 20px;
    font-weight: 700;
    color: var(--color-text-primary);
  }

  .page-subtitle {
    font-size: 12px;
    color: var(--color-text-muted);
    margin-top: 2px;
  }

  .header-right {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .game-tabs {
    display: flex;
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    padding: 2px;
    gap: 2px;
  }

  .game-tab {
    padding: 5px 14px;
    border-radius: calc(var(--radius-md) - 2px);
    font-size: 12px;
    font-family: var(--font-ui);
    color: var(--color-text-secondary);
    background: transparent;
    border: none;
    cursor: pointer;
    transition: all 0.15s;
  }

  .game-tab.active {
    background: var(--color-bg-elevated);
    color: var(--color-accent);
  }

  .btn-icon {
    width: 30px;
    height: 30px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all 0.15s;
  }

  .btn-icon:hover {
    color: var(--color-text-primary);
    border-color: var(--color-accent);
  }

  /* ─── Export ─────────────────────────────────────────────────────────────── */
  .export-wrap {
    position: relative;
  }

  .btn-export {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 14px;
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    color: var(--color-text-secondary);
    font-size: 12px;
    font-family: var(--font-ui);
    cursor: pointer;
    transition: all 0.15s;
  }

  .btn-export:not(.disabled):hover {
    border-color: var(--color-accent);
    color: var(--color-accent);
  }

  .btn-export.disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .export-menu {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    background: var(--color-bg-elevated);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    padding: 4px;
    z-index: 100;
    min-width: 140px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
  }

  .export-menu button {
    display: block;
    width: 100%;
    padding: 7px 12px;
    text-align: left;
    font-size: 13px;
    font-family: var(--font-ui);
    color: var(--color-text-secondary);
    background: transparent;
    border: none;
    border-radius: calc(var(--radius-md) - 2px);
    cursor: pointer;
    transition: background 0.1s;
  }

  .export-menu button:hover {
    background: var(--color-bg-surface);
    color: var(--color-text-primary);
  }

  /* ─── Data source panel ──────────────────────────────────────────────────── */
  .data-panel {
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    padding: 20px 24px;
    flex-shrink: 0;
  }

  .data-panel-title {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text-primary);
    margin-bottom: 8px;
  }

  .data-panel-hint {
    font-size: 12px;
    color: var(--color-text-muted);
    line-height: 1.6;
    margin-bottom: 16px;
  }

  .scan-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .btn-scan {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 7px 16px;
    background: var(--color-bg-elevated);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    color: var(--color-text-secondary);
    font-size: 13px;
    font-family: var(--font-ui);
    cursor: pointer;
    transition: all 0.15s;
    white-space: nowrap;
    flex-shrink: 0;
  }

  .btn-scan:not(:disabled):hover {
    border-color: var(--color-accent);
    color: var(--color-accent);
  }

  .btn-scan:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .scan-sep {
    font-size: 12px;
    color: var(--color-text-muted);
    flex-shrink: 0;
  }

  .url-input-wrap {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    background: var(--color-bg-primary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    padding: 0 12px;
    min-width: 0;
  }

  .url-input-wrap :global(.url-icon) {
    color: var(--color-text-muted);
    flex-shrink: 0;
  }

  .url-input {
    flex: 1;
    background: transparent;
    border: none;
    outline: none;
    color: var(--color-text-primary);
    font-size: 12px;
    font-family: var(--font-mono);
    padding: 8px 0;
    min-width: 0;
  }

  .url-input::placeholder {
    color: var(--color-text-muted);
    font-family: var(--font-ui);
  }

  .btn-fetch {
    padding: 7px 18px;
    background: var(--color-accent);
    border: none;
    border-radius: var(--radius-md);
    color: #0d0e14;
    font-size: 13px;
    font-weight: 600;
    font-family: var(--font-ui);
    cursor: pointer;
    white-space: nowrap;
    flex-shrink: 0;
    transition: opacity 0.15s;
  }

  .btn-fetch:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .status-msg {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 12px;
    font-size: 12px;
    color: var(--color-text-muted);
  }

  .status-msg.ok {
    color: #4ade80;
  }

  .status-msg.error {
    color: #f87171;
  }

  /* ─── Stats grid ─────────────────────────────────────────────────────────── */
  .stats-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 12px;
    flex-shrink: 0;
  }

  .stat-card {
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    padding: 16px 20px;
  }

  .stat-card.accent {
    border-color: rgba(232, 201, 122, 0.3);
    background: rgba(232, 201, 122, 0.04);
  }

  .stat-label {
    font-size: 11px;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    margin-bottom: 6px;
  }

  .stat-value {
    font-size: 28px;
    font-weight: 700;
    color: var(--color-text-primary);
    font-family: var(--font-mono);
    line-height: 1;
    margin-bottom: 4px;
  }

  .stat-card.accent .stat-value {
    color: var(--color-accent);
  }

  .stat-value.pity-warn {
    color: #fbbf24;
  }

  .stat-value.pity-danger {
    color: #f87171;
  }

  .stat-sub {
    font-size: 11px;
    color: var(--color-text-muted);
  }

  /* ─── Pool detail bar ────────────────────────────────────────────────────── */
  .pool-detail {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 16px;
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    font-size: 12px;
    flex-shrink: 0;
  }

  .pd-item {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--color-text-secondary);
  }

  .pd-key {
    color: var(--color-text-muted);
  }

  .pd-sep {
    color: var(--color-border);
  }

  /* ─── Pool tabs ──────────────────────────────────────────────────────────── */
  .pool-tabs {
    display: flex;
    gap: 4px;
    flex-shrink: 0;
  }

  .pool-tab {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 14px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    font-size: 13px;
    font-family: var(--font-ui);
    color: var(--color-text-muted);
    cursor: pointer;
    transition: all 0.15s;
  }

  .pool-tab:hover {
    color: var(--color-text-secondary);
    background: var(--color-bg-surface);
  }

  .pool-tab.active {
    background: var(--color-bg-surface);
    border-color: var(--color-border);
    color: var(--color-accent);
  }

  .tab-count {
    font-size: 11px;
    padding: 1px 6px;
    background: var(--color-bg-elevated);
    border-radius: 10px;
    color: var(--color-text-muted);
  }

  .pool-tab.active .tab-count {
    background: rgba(232, 201, 122, 0.15);
    color: var(--color-accent);
  }

  /* ─── Record table ───────────────────────────────────────────────────────── */
  .records-wrap {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    overflow: hidden;
  }

  .records-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }

  .records-table th {
    padding: 10px 14px;
    text-align: left;
    font-size: 11px;
    font-weight: 600;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    border-bottom: 1px solid var(--color-border);
    background: var(--color-bg-elevated);
    white-space: nowrap;
  }

  .records-table td {
    padding: 7px 14px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.03);
    color: var(--color-text-secondary);
  }

  .records-table tr:last-child td {
    border-bottom: none;
  }

  .records-table tr:hover td {
    background: rgba(255, 255, 255, 0.02);
  }

  /* Rarity row coloring */
  .records-table tr.r6 td { color: var(--color-text-primary); }
  .records-table tr.r6 .td-name { color: #f59e0b; font-weight: 600; }
  .records-table tr.r6 .td-rarity { color: #f59e0b; }

  .records-table tr.r5 .td-name { color: #a78bfa; }
  .records-table tr.r5 .td-rarity { color: #a78bfa; }

  .records-table tr.r4 .td-name { color: #60a5fa; }
  .records-table tr.r4 .td-rarity { color: #60a5fa; }

  .records-table tr.r3 td { color: var(--color-text-muted); }

  .td-time {
    font-family: var(--font-mono);
    font-size: 11px;
    white-space: nowrap;
  }

  .td-pool {
    max-width: 160px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--color-text-muted);
    font-size: 11px;
  }

  .td-rarity {
    font-size: 11px;
    letter-spacing: -1px;
    white-space: nowrap;
  }

  .td-pity {
    font-family: var(--font-mono);
    font-size: 12px;
    text-align: center;
  }

  .td-pity.high-pity {
    color: #fbbf24;
  }

  .td-new {
    text-align: center;
    color: #4ade80;
    font-size: 11px;
  }

  /* ─── Pagination ─────────────────────────────────────────────────────────── */
  .pagination {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 10px;
    border-top: 1px solid var(--color-border);
    flex-shrink: 0;
  }

  .pg-btn {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--color-bg-elevated);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all 0.15s;
  }

  .pg-btn:disabled {
    opacity: 0.3;
    cursor: not-allowed;
  }

  .pg-btn:not(:disabled):hover {
    border-color: var(--color-accent);
    color: var(--color-accent);
  }

  .pg-info {
    font-size: 12px;
    color: var(--color-text-muted);
    font-family: var(--font-mono);
  }

  /* ─── Misc ───────────────────────────────────────────────────────────────── */
  .empty-pool {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-text-muted);
    font-size: 13px;
    padding: 40px;
  }

  .empty-state {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    color: var(--color-text-muted);
    opacity: 0.4;
    font-size: 13px;
  }

  .footer-info {
    font-size: 11px;
    color: var(--color-text-muted);
    text-align: center;
    flex-shrink: 0;
    padding-bottom: 4px;
  }

  .overlay {
    position: fixed;
    inset: 0;
    z-index: 99;
  }
</style>
