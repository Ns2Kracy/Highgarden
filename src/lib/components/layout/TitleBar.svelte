<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { Minus, Square, X } from 'lucide-svelte';

  const appWindow = getCurrentWindow();

  async function minimize() {
    await appWindow.minimize();
  }
  async function toggleMaximize() {
    await appWindow.toggleMaximize();
  }
  async function close() {
    await appWindow.close();
  }
</script>

<div class="titlebar" data-tauri-drag-region>
  <div class="titlebar-left" data-tauri-drag-region>
    <span class="titlebar-logo">HG</span>
    <span class="titlebar-title">HIGHGARDEN</span>
  </div>

  <div class="titlebar-controls">
    <button class="ctrl-btn minimize" onclick={minimize} title="最小化">
      <Minus size={12} />
    </button>
    <button class="ctrl-btn maximize" onclick={toggleMaximize} title="最大化">
      <Square size={11} />
    </button>
    <button class="ctrl-btn close" onclick={close} title="关闭">
      <X size={12} />
    </button>
  </div>
</div>

<style>
  .titlebar {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    height: var(--titlebar-height);
    background: var(--color-bg-primary);
    border-bottom: 1px solid var(--color-border);
    display: flex;
    align-items: center;
    justify-content: space-between;
    z-index: 1000;
    padding: 0 0 0 12px;
  }

  .titlebar-left {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .titlebar-logo {
    font-size: 12px;
    font-weight: 700;
    color: var(--color-accent);
    background: var(--color-accent-glow);
    border: 1px solid var(--color-accent-dim);
    padding: 2px 6px;
    border-radius: var(--radius-sm);
    letter-spacing: 0.05em;
  }

  .titlebar-title {
    font-size: 11px;
    font-weight: 600;
    color: var(--color-text-muted);
    letter-spacing: 0.15em;
  }

  .titlebar-controls {
    display: flex;
    align-items: stretch;
    height: 100%;
  }

  .ctrl-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 46px;
    height: 100%;
    border: none;
    background: transparent;
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: background 0.1s ease, color 0.1s ease;
  }

  .ctrl-btn:hover {
    background: var(--color-bg-elevated);
    color: var(--color-text-primary);
  }

  .ctrl-btn.close:hover {
    background: #c0392b;
    color: #fff;
  }
</style>
