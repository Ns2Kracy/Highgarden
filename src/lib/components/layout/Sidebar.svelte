<script lang="ts">
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import { Library, Sparkles, Settings } from 'lucide-svelte';

  const navItems = [
    { href: '/', icon: Library, label: '游戏库' },
    { href: '/gacha', icon: Sparkles, label: '寻访分析' },
    { href: '/settings', icon: Settings, label: '设置' },
  ];

  function isActive(href: string) {
    if (href === '/') return $page.url.pathname === '/';
    return $page.url.pathname.startsWith(href);
  }
</script>

<nav class="sidebar">
  <ul class="nav-list">
    {#each navItems as item}
      {@const active = isActive(item.href)}
      <li>
        <button
          class="nav-item"
          class:active
          onclick={() => goto(item.href)}
          title={item.label}
        >
          <div class="nav-icon">
            <item.icon size={20} />
          </div>
          {#if active}
            <div class="active-indicator"></div>
          {/if}
        </button>
      </li>
    {/each}
  </ul>
</nav>

<style>
  .sidebar {
    position: fixed;
    top: var(--titlebar-height);
    left: 0;
    bottom: 0;
    width: var(--sidebar-width);
    background: var(--color-bg-secondary);
    border-right: 1px solid var(--color-border);
    display: flex;
    flex-direction: column;
    z-index: 100;
  }

  .nav-list {
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 8px 0;
  }

  .nav-item {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 48px;
    border: none;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    transition: color 0.15s ease, background 0.15s ease;
  }

  .nav-item:hover {
    color: var(--color-text-secondary);
    background: var(--color-bg-elevated);
  }

  .nav-item.active {
    color: var(--color-accent);
  }

  .nav-icon {
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .active-indicator {
    position: absolute;
    right: 0;
    top: 50%;
    transform: translateY(-50%);
    width: 2px;
    height: 24px;
    background: var(--color-accent);
    border-radius: 2px 0 0 2px;
  }
</style>
