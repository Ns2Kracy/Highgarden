import { writable } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';
import type { AppSettings } from '$lib/types';

export const defaults: AppSettings = {
  theme: 'dark',
  language: 'zh-CN',
  downloadConcurrency: 8,
  downloadThreads: 3,
  downloadPath: '',
  proxyUrl: null,
};

export const settings = writable<AppSettings>(defaults);

let _saveTimer: ReturnType<typeof setTimeout> | null = null;
let _initialized = false;

/** Called once after config is loaded; subsequent changes auto-persist. */
export function markInitialized() {
  _initialized = true;
}

settings.subscribe((value) => {
  if (!_initialized) return;
  if (_saveTimer) clearTimeout(_saveTimer);
  _saveTimer = setTimeout(async () => {
    try {
      await invoke('set_settings', { settings: value });
    } catch (e) {
      console.warn('Failed to save settings:', e);
    }
  }, 500);
});
