<script lang="ts">
  import '../app.css';
  import TitleBar from '$lib/components/layout/TitleBar.svelte';
  import Sidebar from '$lib/components/layout/Sidebar.svelte';
  import { onMount } from 'svelte';
  import { settings, defaults, markInitialized } from '$lib/stores/settings';
  import { games, updateGame } from '$lib/stores/games';
  import { get } from 'svelte/store';
  import { invoke } from '@tauri-apps/api/core';
  import type { AppSettings, GameId } from '$lib/types';

  let { children } = $props();

  onMount(async () => {
    try {
      const config = await invoke<{
        settings: AppSettings;
        gamePaths: Record<string, string>;
      }>('get_app_config');

      settings.set({ ...defaults, ...config.settings });

      if (config.gamePaths && Object.keys(config.gamePaths).length > 0) {
        const updatedGames = await Promise.all(
          get(games).map(async (g) => {
            const path = config.gamePaths[g.id];
            if (!path) return g;
            const installed = await invoke<boolean>('validate_game_path', {
              gameId: g.id,
              path
            }).catch(() => false);
            return { ...g, installPath: path, installed };
          })
        );
        games.set(updatedGames);
      }
    } catch (e) {
      console.warn('Failed to load config:', e);
    } finally {
      markInitialized();
    }

    // Background: check for updates for each installed game.
    for (const g of get(games)) {
      if (!g.installed || !g.installPath) continue;
      invoke<{ localVersion: string | null; latestVersion: string | null; updateAvailable: boolean }>(
        'check_game_update',
        { gameId: g.id, installPath: g.installPath }
      )
        .then((r) => {
          updateGame(g.id as GameId, {
            version: r.localVersion ?? undefined,
            latestVersion: r.latestVersion ?? undefined,
            updateAvailable: r.updateAvailable,
          });
        })
        .catch(() => {});
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
