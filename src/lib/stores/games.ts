import { writable, derived } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';
import type { Game, GameId } from '$lib/types';

const defaultGames: Game[] = [
  {
    id: 'arknights',
    name: '明日方舟',
    nameEn: 'Arknights',
    installPath: null,
    installed: false,
    version: null,
    latestVersion: null,
    updateAvailable: false,
    backgroundImage: '/images/ak-bg.jpg',
    icon: '/images/ak-icon.png',
  },
  {
    id: 'endfield',
    name: '明日方舟：终末地',
    nameEn: 'Arknights: Endfield',
    installPath: null,
    installed: false,
    version: null,
    latestVersion: null,
    updateAvailable: false,
    backgroundImage: '/images/endfield-bg.jpg',
    icon: '/images/endfield-icon.png',
  },
];

export const games = writable<Game[]>(defaultGames);
export const selectedGameId = writable<GameId>('arknights');

export const selectedGame = derived(
  [games, selectedGameId],
  ([$games, $selectedGameId]) => $games.find((g) => g.id === $selectedGameId) ?? $games[0]
);

export function updateGame(id: GameId, patch: Partial<Game>) {
  games.update((gs) => gs.map((g) => (g.id === id ? { ...g, ...patch } : g)));

  // Persist install path changes to Rust immediately
  if ('installPath' in patch) {
    invoke('set_game_path', { gameId: id, path: patch.installPath ?? null }).catch((e) =>
      console.warn('Failed to save game path:', e)
    );
  }
}
