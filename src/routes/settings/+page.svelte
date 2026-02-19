<script lang="ts">
  import { settings } from '$lib/stores/settings';
  import { invoke } from '@tauri-apps/api/core';
  import { FolderOpen } from 'lucide-svelte';

  async function selectDownloadPath() {
    try {
      const path = await invoke<string | null>('select_download_path');
      if (path) {
        settings.update((s) => ({ ...s, downloadPath: path }));
      }
    } catch (e) {
      console.error(e);
    }
  }
</script>

<div class="settings-page">
  <div class="page-header">
    <h2 class="page-title">设置</h2>
  </div>

  <div class="settings-sections">
    <!-- Download settings -->
    <section class="settings-section">
      <h3 class="section-title">下载设置</h3>

      <div class="setting-row">
        <div class="setting-label">
          <span>默认下载路径</span>
          <p>游戏文件的保存位置</p>
        </div>
        <div class="setting-control path-control">
          <span class="path-text">{$settings.downloadPath || '未设置'}</span>
          <button class="btn-sm" onclick={selectDownloadPath}>
            <FolderOpen size={14} />
            <span>选择</span>
          </button>
        </div>
      </div>

      <div class="setting-row">
        <div class="setting-label">
          <span>代理地址</span>
          <p>HTTP/HTTPS 代理服务器 (可选)</p>
        </div>
        <div class="setting-control">
          <input
            type="text"
            class="text-input"
            placeholder="http://127.0.0.1:7890"
            bind:value={$settings.proxyUrl}
          />
        </div>
      </div>
    </section>

    <!-- About -->
    <section class="settings-section">
      <h3 class="section-title">关于</h3>
      <div class="about-card">
        <div class="about-logo">HG</div>
        <div class="about-info">
          <span class="about-name">Highgarden</span>
          <span class="about-version">v0.1.0</span>
          <p class="about-desc">鹰角网络游戏第三方启动器，支持明日方舟与明日方舟：终末地</p>
        </div>
      </div>
    </section>
  </div>
</div>

<style>
  .settings-page {
    height: 100%;
    display: flex;
    flex-direction: column;
    padding: 32px;
    gap: 24px;
    overflow-y: auto;
  }

  .page-header {
    flex-shrink: 0;
  }

  .page-title {
    font-size: 22px;
    font-weight: 700;
    color: var(--color-text-primary);
  }

  .settings-sections {
    display: flex;
    flex-direction: column;
    gap: 32px;
    max-width: 680px;
  }

  .settings-section {
    display: flex;
    flex-direction: column;
    gap: 0;
  }

  .section-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--color-accent);
    letter-spacing: 0.12em;
    text-transform: uppercase;
    padding-bottom: 12px;
    border-bottom: 1px solid var(--color-border);
    margin-bottom: 0;
  }

  .setting-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
    padding: 14px 0;
    border-bottom: 1px solid var(--color-border);
  }

  .setting-label {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
  }

  .setting-label span {
    font-size: 14px;
    color: var(--color-text-primary);
    font-weight: 500;
  }

  .setting-label p {
    font-size: 12px;
    color: var(--color-text-muted);
  }

  .setting-control {
    flex-shrink: 0;
  }

  .path-control {
    display: flex;
    align-items: center;
    gap: 8px;
    max-width: 280px;
  }

  .path-text {
    font-size: 12px;
    color: var(--color-text-muted);
    font-family: var(--font-mono);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 200px;
  }

  .btn-sm {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 6px 12px;
    background: var(--color-bg-elevated);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    color: var(--color-text-secondary);
    font-size: 12px;
    font-family: var(--font-ui);
    cursor: pointer;
    white-space: nowrap;
    transition: all 0.1s;
  }

  .btn-sm:hover {
    border-color: var(--color-border-hover);
    color: var(--color-text-primary);
  }

  .text-input {
    background: var(--color-bg-elevated);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    color: var(--color-text-primary);
    font-size: 13px;
    font-family: var(--font-mono);
    padding: 6px 10px;
    outline: none;
    transition: border-color 0.1s;
    width: 240px;
  }

  .text-input:focus {
    border-color: var(--color-accent-dim);
  }

  /* About */
  .about-card {
    display: flex;
    align-items: center;
    gap: 20px;
    padding: 20px 0;
  }

  .about-logo {
    font-size: 16px;
    font-weight: 700;
    color: var(--color-accent);
    background: var(--color-accent-glow);
    border: 1px solid var(--color-accent-dim);
    width: 48px;
    height: 48px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--radius-md);
    flex-shrink: 0;
  }

  .about-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .about-name {
    font-size: 16px;
    font-weight: 700;
    color: var(--color-text-primary);
  }

  .about-version {
    font-size: 12px;
    color: var(--color-text-muted);
    font-family: var(--font-mono);
  }

  .about-desc {
    font-size: 12px;
    color: var(--color-text-muted);
    margin-top: 4px;
    line-height: 1.5;
  }
</style>
