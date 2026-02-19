<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import {
    FileDown,
    RefreshCw,
    ChevronLeft,
    ChevronRight,
    LogOut,
    Phone,
    Lock,
    Hash,
    Loader,
    CheckCircle2,
    AlertCircle,
  } from 'lucide-svelte';
  import type { GameId } from '$lib/types';

  // ─── Local types ─────────────────────────────────────────────────────────────

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

  interface SessionInfo {
    phoneMasked: string;
    uid: string;
  }

  // ─── State ───────────────────────────────────────────────────────────────────

  let selectedGame = $state<GameId>('arknights');

  // Auth
  let session = $state<SessionInfo | null>(null);
  let loginTab = $state<'password' | 'sms'>('password');
  let phone = $state('');
  let password = $state('');
  let smsCode = $state('');
  let smsSent = $state(false);
  let smsCooldown = $state(0); // countdown seconds
  let authLoading = $state(false);
  let authError = $state('');

  // Records
  let gachaData = $state<GachaData | null>(null);
  let stats = $state<GachaStats | null>(null);
  let fetchLoading = $state(false);
  let fetchMsg = $state('');
  let fetchOk = $state(false);

  // Table
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
    selectedPool === 'all' || !stats ? null : (stats.byPool[selectedPool] ?? null)
  );

  let overallSixStarCount = $derived(
    Object.values(stats?.byPool ?? {}).reduce((s, p) => s + p.sixStarCount, 0)
  );

  let overallSixStarRate = $derived(
    stats && stats.totalPulls > 0
      ? ((overallSixStarCount / stats.totalPulls) * 100).toFixed(1)
      : '0.0'
  );

  let overallCurrentPity = $derived(
    stats ? Math.max(...Object.values(stats.byPool).map((p) => p.currentPity), 0) : 0
  );

  let overallAvgPity = $derived(() => {
    if (!stats) return '0';
    const pools = Object.values(stats.byPool).filter((p) => p.sixStarCount > 0);
    if (!pools.length) return '0';
    return (pools.reduce((s, p) => s + p.avgPity, 0) / pools.length).toFixed(1);
  });

  // ─── Lifecycle ───────────────────────────────────────────────────────────────

  onMount(async () => {
    await loadSession();
    await loadLocalData();
  });

  $effect(() => {
    // eslint-disable-next-line @typescript-eslint/no-unused-expressions
    selectedPool;
    currentPage = 1;
  });

  $effect(() => {
    // eslint-disable-next-line @typescript-eslint/no-unused-expressions
    selectedGame;
    gachaData = null;
    stats = null;
    fetchMsg = '';
    fetchOk = false;
    loadLocalData();
  });

  // ─── Auth actions ─────────────────────────────────────────────────────────────

  async function loadSession() {
    try {
      session = await invoke<SessionInfo | null>('get_hypergryph_session');
    } catch {
      session = null;
    }
  }

  async function loginPassword() {
    if (!phone.trim() || !password.trim()) return;
    authLoading = true;
    authError = '';
    try {
      session = await invoke<SessionInfo>('hypergryph_login_password', {
        phone: phone.trim(),
        password: password.trim(),
      });
      password = '';
    } catch (e) {
      authError = String(e);
    } finally {
      authLoading = false;
    }
  }

  async function sendSms() {
    if (!phone.trim()) { authError = '请输入手机号'; return; }
    authLoading = true;
    authError = '';
    try {
      await invoke('hypergryph_send_sms', { phone: phone.trim() });
      smsSent = true;
      smsCooldown = 60;
      const timer = setInterval(() => {
        smsCooldown -= 1;
        if (smsCooldown <= 0) clearInterval(timer);
      }, 1000);
    } catch (e) {
      authError = String(e);
    } finally {
      authLoading = false;
    }
  }

  async function loginBySms() {
    if (!phone.trim() || !smsCode.trim()) return;
    authLoading = true;
    authError = '';
    try {
      session = await invoke<SessionInfo>('hypergryph_login_by_code', {
        phone: phone.trim(),
        code: smsCode.trim(),
      });
      smsCode = '';
      smsSent = false;
    } catch (e) {
      authError = String(e);
    } finally {
      authLoading = false;
    }
  }

  async function logout() {
    try {
      await invoke('hypergryph_logout');
      session = null;
    } catch {
      session = null;
    }
  }

  // ─── Gacha fetch ──────────────────────────────────────────────────────────────

  async function loadLocalData() {
    try {
      const data = await invoke<GachaData | null>('get_local_gacha_records', {
        gameId: selectedGame,
      });
      if (data) {
        gachaData = data;
        stats = await invoke<GachaStats | null>('get_gacha_stats', { gameId: selectedGame });
      }
    } catch {
      // No local data yet
    }
  }

  async function fetchGacha() {
    fetchLoading = true;
    fetchMsg = '正在获取记录，请稍候…';
    fetchOk = false;
    try {
      const result = await invoke<{ uid: string; total: number }>('fetch_gacha_with_login', {
        gameId: selectedGame,
      });
      fetchMsg = `获取完成：共 ${result.total} 条记录（UID ${result.uid}）`;
      fetchOk = true;
      await loadLocalData();
    } catch (e) {
      fetchMsg = String(e);
      fetchOk = false;
    } finally {
      fetchLoading = false;
    }
  }

  // ─── Export ───────────────────────────────────────────────────────────────────

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
      fetchMsg = `已导出至 ${path.split('\\').pop() ?? path}`;
      fetchOk = true;
    } catch (e) {
      fetchMsg = String(e);
      fetchOk = false;
    }
  }

  // ─── Helpers ─────────────────────────────────────────────────────────────────

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

      {#if session && hasRecords}
        <button
          class="btn-icon"
          title="刷新记录"
          disabled={fetchLoading}
          onclick={fetchGacha}
        >
          <RefreshCw size={14} class={fetchLoading ? 'spin' : ''} />
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
          <FileDown size={14} />
          <span>导出</span>
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

  <!-- ── Auth / login section ── -->
  {#if !session}
    <div class="auth-card">
      <div class="auth-title">登录鹰角通行证</div>
      <p class="auth-hint">登录后可直接获取寻访记录，无需手动操作</p>

      <!-- Login method tabs -->
      <div class="login-tabs">
        <button
          class="login-tab"
          class:active={loginTab === 'password'}
          onclick={() => { loginTab = 'password'; authError = ''; }}
        >
          密码登录
        </button>
        <button
          class="login-tab"
          class:active={loginTab === 'sms'}
          onclick={() => { loginTab = 'sms'; authError = ''; }}
        >
          验证码登录
        </button>
      </div>

      <div class="login-form">
        <!-- Phone -->
        <div class="field">
          <div class="field-icon"><Phone size={14} /></div>
          <input
            class="field-input"
            type="tel"
            placeholder="手机号码"
            bind:value={phone}
            maxlength={11}
          />
        </div>

        {#if loginTab === 'password'}
          <!-- Password -->
          <div class="field">
            <div class="field-icon"><Lock size={14} /></div>
            <input
              class="field-input"
              type="password"
              placeholder="账号密码"
              bind:value={password}
              onkeydown={(e) => e.key === 'Enter' && loginPassword()}
            />
          </div>

          <button
            class="btn-login"
            disabled={authLoading || !phone.trim() || !password.trim()}
            onclick={loginPassword}
          >
            {#if authLoading}
              <Loader size={14} class="spin" />
              <span>登录中…</span>
            {:else}
              <span>登录</span>
            {/if}
          </button>
        {:else}
          <!-- SMS code -->
          <div class="field">
            <div class="field-icon"><Hash size={14} /></div>
            <input
              class="field-input"
              type="text"
              placeholder="短信验证码"
              bind:value={smsCode}
              maxlength={6}
              onkeydown={(e) => e.key === 'Enter' && loginBySms()}
            />
            <button
              class="btn-send-sms"
              disabled={authLoading || smsCooldown > 0 || !phone.trim()}
              onclick={sendSms}
            >
              {smsCooldown > 0 ? `${smsCooldown}s` : '发送'}
            </button>
          </div>

          <button
            class="btn-login"
            disabled={authLoading || !phone.trim() || !smsCode.trim()}
            onclick={loginBySms}
          >
            {#if authLoading}
              <Loader size={14} class="spin" />
              <span>登录中…</span>
            {:else}
              <span>登录</span>
            {/if}
          </button>
        {/if}

        {#if authError}
          <div class="auth-error">
            <AlertCircle size={13} />
            <span>{authError}</span>
          </div>
        {/if}
      </div>

      <p class="auth-note">
        账号密码仅用于向鹰角服务器验证身份，本地只保存会话令牌，不存储密码。
      </p>
    </div>
  {:else}
    <!-- ── Logged-in action card ── -->
    <div class="session-card">
      <div class="session-info">
        <div class="session-phone">{session.phoneMasked}</div>
        <div class="session-uid">UID {session.uid}</div>
      </div>
      <div class="session-actions">
        <button
          class="btn-fetch-main"
          disabled={fetchLoading}
          onclick={fetchGacha}
        >
          {#if fetchLoading}
            <Loader size={14} class="spin" />
            <span>获取中…</span>
          {:else}
            <RefreshCw size={14} />
            <span>{hasRecords ? '刷新记录' : '获取记录'}</span>
          {/if}
        </button>
        <button class="btn-logout" onclick={logout} title="退出登录">
          <LogOut size={14} />
        </button>
      </div>
    </div>
  {/if}

  <!-- Fetch status message -->
  {#if fetchMsg}
    <div class="fetch-status" class:ok={fetchOk} class:error={!fetchOk && !fetchLoading}>
      {#if fetchOk}
        <CheckCircle2 size={13} />
      {:else if !fetchLoading}
        <AlertCircle size={13} />
      {/if}
      <span>{fetchMsg}</span>
    </div>
  {/if}

  {#if hasRecords}
    <!-- ── Stats grid ── -->
    <div class="stats-grid">
      <div class="stat-card">
        <div class="stat-label">总抽数</div>
        <div class="stat-value">{stats?.totalPulls.toLocaleString() ?? 0}</div>
        <div class="stat-sub">次寻访</div>
      </div>
      <div class="stat-card accent">
        <div class="stat-label">六星出货率</div>
        <div class="stat-value">
          {currentPoolStats ? currentPoolStats.sixStarRate.toFixed(1) : overallSixStarRate}%
        </div>
        <div class="stat-sub">
          {currentPoolStats ? currentPoolStats.sixStarCount : overallSixStarCount} 个六星
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
          {currentPoolStats ? currentPoolStats.avgPity.toFixed(1) : overallAvgPity()}
        </div>
        <div class="stat-sub">抽 / 六星</div>
      </div>
    </div>

    <!-- Pool sub-stats bar -->
    {#if selectedPool !== 'all' && currentPoolStats}
      <div class="pool-detail">
        <span class="pd-item"><span class="pd-key">总抽</span>{currentPoolStats.totalPulls}</span>
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
        UID {stats.uid} · 最后同步 {formatDate(stats.fetchedAt)}
      </div>
    {/if}
  {:else if session}
    <div class="empty-state">
      <p>点击「获取记录」从服务器同步寻访历史</p>
    </div>
  {/if}
</div>

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
    gap: 14px;
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

  .btn-icon:not(:disabled):hover {
    color: var(--color-text-primary);
    border-color: var(--color-accent);
  }

  .btn-icon:disabled { opacity: 0.4; cursor: not-allowed; }

  /* ─── Export ─────────────────────────────────────────────────────────────── */
  .export-wrap { position: relative; }

  .btn-export {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
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

  .btn-export.disabled { opacity: 0.4; cursor: not-allowed; }

  .export-menu {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    background: var(--color-bg-elevated);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    padding: 4px;
    z-index: 100;
    min-width: 130px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
  }

  .export-menu button {
    display: block;
    width: 100%;
    padding: 7px 12px;
    text-align: left;
    font-size: 12px;
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

  /* ─── Auth card ──────────────────────────────────────────────────────────── */
  .auth-card {
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    padding: 24px 28px;
    flex-shrink: 0;
    max-width: 420px;
    align-self: center;
    width: 100%;
  }

  .auth-title {
    font-size: 15px;
    font-weight: 700;
    color: var(--color-text-primary);
    margin-bottom: 6px;
  }

  .auth-hint {
    font-size: 12px;
    color: var(--color-text-muted);
    margin-bottom: 18px;
    line-height: 1.5;
  }

  .login-tabs {
    display: flex;
    gap: 0;
    border-bottom: 1px solid var(--color-border);
    margin-bottom: 18px;
  }

  .login-tab {
    padding: 6px 16px;
    font-size: 13px;
    font-family: var(--font-ui);
    color: var(--color-text-muted);
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    cursor: pointer;
    transition: all 0.15s;
  }

  .login-tab.active {
    color: var(--color-accent);
    border-bottom-color: var(--color-accent);
  }

  .login-form {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .field {
    display: flex;
    align-items: center;
    gap: 8px;
    background: var(--color-bg-primary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    padding: 0 12px;
    transition: border-color 0.15s;
  }

  .field:focus-within {
    border-color: var(--color-accent);
  }

  .field-icon {
    color: var(--color-text-muted);
    display: flex;
    align-items: center;
    flex-shrink: 0;
  }

  .field-input {
    flex: 1;
    background: transparent;
    border: none;
    outline: none;
    color: var(--color-text-primary);
    font-size: 13px;
    font-family: var(--font-ui);
    padding: 10px 0;
    min-width: 0;
  }

  .field-input::placeholder { color: var(--color-text-muted); }

  .btn-send-sms {
    padding: 4px 10px;
    background: var(--color-bg-elevated);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    color: var(--color-text-secondary);
    font-size: 11px;
    font-family: var(--font-ui);
    cursor: pointer;
    white-space: nowrap;
    flex-shrink: 0;
    transition: all 0.15s;
  }

  .btn-send-sms:not(:disabled):hover {
    border-color: var(--color-accent);
    color: var(--color-accent);
  }

  .btn-send-sms:disabled { opacity: 0.4; cursor: not-allowed; }

  .btn-login {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 10px;
    background: var(--color-accent);
    border: none;
    border-radius: var(--radius-md);
    color: #0d0e14;
    font-size: 14px;
    font-weight: 700;
    font-family: var(--font-ui);
    cursor: pointer;
    margin-top: 4px;
    transition: opacity 0.15s;
  }

  .btn-login:disabled { opacity: 0.5; cursor: not-allowed; }

  .auth-error {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: #f87171;
    padding: 2px 0;
  }

  .auth-note {
    font-size: 11px;
    color: var(--color-text-muted);
    margin-top: 14px;
    line-height: 1.5;
    padding-top: 12px;
    border-top: 1px solid var(--color-border);
  }

  /* ─── Session card (logged-in) ───────────────────────────────────────────── */
  .session-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 20px;
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    flex-shrink: 0;
  }

  .session-info {
    display: flex;
    align-items: center;
    gap: 16px;
  }

  .session-phone {
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text-primary);
    font-family: var(--font-mono);
  }

  .session-uid {
    font-size: 12px;
    color: var(--color-text-muted);
    font-family: var(--font-mono);
  }

  .session-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .btn-fetch-main {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 20px;
    background: var(--color-accent);
    border: none;
    border-radius: var(--radius-md);
    color: #0d0e14;
    font-size: 13px;
    font-weight: 700;
    font-family: var(--font-ui);
    cursor: pointer;
    transition: opacity 0.15s;
  }

  .btn-fetch-main:disabled { opacity: 0.5; cursor: not-allowed; }

  .btn-logout {
    width: 34px;
    height: 34px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    color: var(--color-text-muted);
    cursor: pointer;
    transition: all 0.15s;
  }

  .btn-logout:hover { color: #f87171; border-color: #f87171; }

  /* ─── Fetch status ───────────────────────────────────────────────────────── */
  .fetch-status {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--color-text-muted);
    flex-shrink: 0;
  }

  .fetch-status.ok { color: #4ade80; }
  .fetch-status.error { color: #f87171; }

  /* ─── Stats grid ─────────────────────────────────────────────────────────── */
  .stats-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 10px;
    flex-shrink: 0;
  }

  .stat-card {
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    padding: 14px 18px;
  }

  .stat-card.accent {
    border-color: rgba(232, 201, 122, 0.3);
    background: rgba(232, 201, 122, 0.04);
  }

  .stat-label {
    font-size: 10px;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    margin-bottom: 6px;
  }

  .stat-value {
    font-size: 26px;
    font-weight: 700;
    color: var(--color-text-primary);
    font-family: var(--font-mono);
    line-height: 1;
    margin-bottom: 4px;
  }

  .stat-card.accent .stat-value { color: var(--color-accent); }
  .stat-value.pity-warn { color: #fbbf24; }
  .stat-value.pity-danger { color: #f87171; }

  .stat-sub { font-size: 11px; color: var(--color-text-muted); }

  /* ─── Pool detail bar ────────────────────────────────────────────────────── */
  .pool-detail {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 16px;
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    font-size: 12px;
    flex-shrink: 0;
  }

  .pd-item { display: flex; align-items: center; gap: 4px; color: var(--color-text-secondary); }
  .pd-key { color: var(--color-text-muted); }
  .pd-sep { color: var(--color-border); }

  /* ─── Pool tabs ──────────────────────────────────────────────────────────── */
  .pool-tabs { display: flex; gap: 4px; flex-shrink: 0; }

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

  .pool-tab:hover { color: var(--color-text-secondary); background: var(--color-bg-surface); }
  .pool-tab.active { background: var(--color-bg-surface); border-color: var(--color-border); color: var(--color-accent); }

  .tab-count {
    font-size: 10px;
    padding: 1px 5px;
    background: var(--color-bg-elevated);
    border-radius: 10px;
    color: var(--color-text-muted);
  }

  .pool-tab.active .tab-count { background: rgba(232, 201, 122, 0.15); color: var(--color-accent); }

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

  .records-table { width: 100%; border-collapse: collapse; font-size: 12px; }

  .records-table th {
    padding: 9px 14px;
    text-align: left;
    font-size: 10px;
    font-weight: 600;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    border-bottom: 1px solid var(--color-border);
    background: var(--color-bg-elevated);
    white-space: nowrap;
  }

  .records-table td {
    padding: 6px 14px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.03);
    color: var(--color-text-secondary);
  }

  .records-table tr:last-child td { border-bottom: none; }
  .records-table tr:hover td { background: rgba(255, 255, 255, 0.02); }

  .records-table tr.r6 td { color: var(--color-text-primary); }
  .records-table tr.r6 .td-name { color: #f59e0b; font-weight: 600; }
  .records-table tr.r6 .td-rarity { color: #f59e0b; }
  .records-table tr.r5 .td-name { color: #a78bfa; }
  .records-table tr.r5 .td-rarity { color: #a78bfa; }
  .records-table tr.r4 .td-name { color: #60a5fa; }
  .records-table tr.r4 .td-rarity { color: #60a5fa; }
  .records-table tr.r3 td { color: var(--color-text-muted); }

  .td-time { font-family: var(--font-mono); font-size: 11px; white-space: nowrap; }
  .td-pool { max-width: 160px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--color-text-muted); font-size: 11px; }
  .td-rarity { font-size: 10px; letter-spacing: -1px; white-space: nowrap; }
  .td-pity { font-family: var(--font-mono); font-size: 12px; text-align: center; }
  .td-pity.high-pity { color: #fbbf24; }
  .td-new { text-align: center; color: #4ade80; font-size: 11px; }

  /* ─── Pagination ─────────────────────────────────────────────────────────── */
  .pagination {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 8px;
    border-top: 1px solid var(--color-border);
    flex-shrink: 0;
  }

  .pg-btn {
    width: 26px;
    height: 26px;
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

  .pg-btn:disabled { opacity: 0.3; cursor: not-allowed; }
  .pg-btn:not(:disabled):hover { border-color: var(--color-accent); color: var(--color-accent); }
  .pg-info { font-size: 12px; color: var(--color-text-muted); font-family: var(--font-mono); }

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
    align-items: center;
    justify-content: center;
    color: var(--color-text-muted);
    font-size: 13px;
  }

  .footer-info {
    font-size: 11px;
    color: var(--color-text-muted);
    text-align: center;
    flex-shrink: 0;
  }

  .overlay { position: fixed; inset: 0; z-index: 99; }

  /* ─── Spinner animation ──────────────────────────────────────────────────── */
  :global(.spin) {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
