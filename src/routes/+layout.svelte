<script lang="ts">
  import '../app.css';
  import TitleBar from '$lib/components/layout/TitleBar.svelte';
  import Sidebar from '$lib/components/layout/Sidebar.svelte';
  import { onMount } from 'svelte';
  import { settings, defaults, markInitialized } from '$lib/stores/settings';
  import { games } from '$lib/stores/games';
  import { invoke } from '@tauri-apps/api/core';
  import type { AppSettings } from '$lib/types';

  let { children } = $props();

  onMount(async () => {
    try {
      const config = await invoke<{
        settings: AppSettings;
        gamePaths: Record<string, string>;
      }>('get_app_config');

      settings.set({ ...defaults, ...config.settings });

      if (config.gamePaths && Object.keys(config.gamePaths).length > 0) {
        games.update((gs) =>
          gs.map((g) => {
            const path = config.gamePaths[g.id];
            if (path) return { ...g, installPath: path, installed: true };
            return g;
          })
        );
      }
    } catch (e) {
      console.warn('Failed to load config:', e);
    } finally {
      markInitialized();
    }
  });
</script>

<TitleBar />
<Sidebar />

<main class="main-content">
  {@render children()}
</main>

<style>
  .main-content {
    position: fixed;
    top: var(--titlebar-height);
    left: var(--sidebar-width);
    right: 0;
    bottom: 0;
    overflow: hidden;
    background: var(--color-bg-primary);
  }
</style>
