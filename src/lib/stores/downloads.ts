import { writable } from 'svelte/store';
import type { DownloadTask } from '$lib/types';

export const downloadTasks = writable<DownloadTask[]>([]);

export function addTask(task: DownloadTask) {
  downloadTasks.update((tasks) => {
    // Avoid duplicates
    if (tasks.some(t => t.id === task.id)) return tasks;
    return [...tasks, task];
  });
}

export function updateTask(id: string, patch: Partial<DownloadTask>) {
  downloadTasks.update((tasks) =>
    tasks.map((t) => (t.id === id ? { ...t, ...patch } : t))
  );
}

export function removeTask(id: string) {
  downloadTasks.update((tasks) => tasks.filter((t) => t.id !== id));
}
