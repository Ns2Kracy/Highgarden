// Game types
export type GameId = 'arknights' | 'endfield';

export interface Game {
  id: GameId;
  name: string;
  nameEn: string;
  installPath: string | null;
  installed: boolean;
  version: string | null;
  latestVersion: string | null;
  updateAvailable: boolean;
  backgroundImage: string;
  icon: string;
}

// Hypergryph game manifest (from launcher API)
export interface GamePack {
  url: string;
  md5: string;
  size: number;
  filename: string;
}

export interface GameManifest {
  gameId: string;
  version: string;
  packs: GamePack[];
  totalSize: number;
  gameFilesMd5: string;
  filePath: string;
}

// Download types
export type DownloadStatus = 'pending' | 'downloading' | 'paused' | 'verifying' | 'completed' | 'error';

export interface DownloadTask {
  id: string;
  gameId: GameId;
  name: string;
  totalSize: number;
  downloadedSize: number;
  progress: number; // 0-100
  speed: number; // bytes/s
  status: DownloadStatus;
  error: string | null;
  createdAt: number;
}

export interface DownloadProgress {
  taskId: string;
  downloadedSize: number;
  totalSize: number;
  progress: number;
  speed: number;
  status: DownloadStatus;
  error: string | null;
}

// Gacha types
export type GachaPoolType = 'standard' | 'limited' | 'beginner' | 'special';

export interface GachaRecord {
  id: string;
  uid: string;
  gameId: GameId;
  poolName: string;
  poolType: GachaPoolType;
  itemName: string;
  itemType: 'character' | 'weapon';
  rarity: number;
  timestamp: number;
  pity: number;
}

export interface GachaStats {
  gameId: GameId;
  poolType: GachaPoolType;
  totalPulls: number;
  sixStarCount: number;
  fiveStarCount: number;
  fourStarCount: number;
  threeStarCount: number;
  sixStarRate: number;
  currentPity: number;
  avgPity: number;
  records: GachaRecord[];
}

// App settings
export interface AppSettings {
  theme: 'dark' | 'light';
  language: 'zh-CN' | 'en-US';
  downloadConcurrency: number;
  downloadThreads: number;
  downloadPath: string;
  proxyUrl: string | null;
}
