import { invoke } from '@tauri-apps/api/core';
import type {
  AddRequest,
  Bootstrap,
  BridgeStatus,
  Browser,
  BrowserCookie,
  Download,
  FeedView,
  FilterStatus,
  GlobalStats,
  MediaInfo,
  Pending,
  ProbeInfo,
  ReadyUpdate,
  MandatoryUpdate,
  SessionStatus,
  Settings,
  ToolStatus,
  TorrentDetails,
  TorrentPreview,
  UpdateReport
} from '../types';

export const api = {
  bootstrap: () => invoke<Bootstrap>('get_bootstrap'),
  downloads: () => invoke<Download[]>('get_downloads'),
  globalStats: () => invoke<GlobalStats>('get_global_stats'),
  probeUrl: (url: string, referrer?: string | null, cookies?: BrowserCookie[]) =>
    invoke<ProbeInfo>('probe_url', { url, referrer, cookies }),
  mediaProbe: (url: string, cookies?: BrowserCookie[]) => invoke<MediaInfo>('media_probe', { url, cookies }),
  torrentInspect: (source: string, file?: string | null) =>
    invoke<TorrentPreview>('torrent_inspect', { source, file }),
  add: (request: AddRequest) => invoke<string>('add_download', { request }),
  pause: (id: string) => invoke<void>('pause_download', { id }),
  resume: (id: string) => invoke<void>('resume_download', { id }),
  remove: (id: string, deleteFiles: boolean) => invoke<void>('remove_download', { id, deleteFiles }),
  removeMany: (ids: string[], deleteFiles: boolean) => invoke<number>('remove_downloads', { ids, deleteFiles }),
  pauseAll: () => invoke<void>('pause_all'),
  resumeAll: () => invoke<void>('resume_all'),
  clearFinished: () => invoke<void>('clear_finished'),
  setThreads: (id: string, threads: number) => invoke<void>('set_threads', { id, threads }),
  setSpeedLimit: (id: string, kib: number) => invoke<void>('set_speed_limit', { id, kib }),
  downloadAppUpdate: () => invoke<ReadyUpdate | null>('download_app_update'),
  appUpdateReady: () => invoke<ReadyUpdate | null>('app_update_ready'),
  mandatoryUpdateStatus: () => invoke<MandatoryUpdate | null>('mandatory_update_status'),
  retryMandatoryUpdate: () => invoke<void>('retry_mandatory_update'),
  installAppUpdate: () => invoke<void>('install_app_update'),
  smartscreenStatus: () => invoke<'on' | 'off' | 'missing' | 'unsupported'>('smartscreen_status'),
  torrentRecheck: (id: string) => invoke<void>('torrent_recheck', { id }),
  torrentExport: (id: string, dest: string) => invoke<void>('torrent_export', { id, dest }),
  createTorrent: (source: string, trackers: string[], pieceKib: number | null, saveTo: string, seed: boolean) =>
    invoke<string>('create_torrent', { source, trackers, pieceKib, saveTo, seed }),
  torrentStreamUrl: (id: string, file: number) => invoke<string>('torrent_stream_url', { id, file }),
  rssFeeds: () => invoke<FeedView[]>('rss_feeds'),
  rssRefresh: () => invoke<number>('rss_refresh'),
  rssDownload: (feed: string, item: string) => invoke<string>('rss_download', { feed, item }),
  moveInQueue: (id: string, to: 'up' | 'down' | 'top' | 'bottom') => invoke<void>('move_in_queue', { id, to }),
  setExpectedHash: (id: string, hash: string | null) => invoke<void>('set_expected_hash', { id, hash }),
  openFile: (id: string) => invoke<void>('open_file', { id }),
  showInFolder: (id: string) => invoke<void>('show_in_folder', { id }),
  openPath: (path: string) => invoke<void>('open_path', { path }),
  openUrl: (url: string) => invoke<void>('open_url', { url }),
  torrentDetails: (id: string) => invoke<TorrentDetails>('torrent_details', { id }),
  torrentSetFiles: (id: string, files: number[]) => invoke<void>('torrent_set_files', { id, files }),
  torrentSessionStatus: () =>
    invoke<{ status: SessionStatus; error: string | null }>('torrent_session_status'),
  startTorrentEngine: () => invoke<void>('start_torrent_engine'),
  publicTrackers: () => invoke<string[]>('public_trackers'),
  getSettings: () => invoke<Settings>('get_settings'),
  saveSettings: (settings: Settings) => invoke<Settings>('save_settings', { settings }),
  toolsStatus: () => invoke<ToolStatus>('tools_status'),
  updateYtdlp: () => invoke<string>('update_ytdlp'),
  installFfmpeg: () => invoke<void>('install_ffmpeg'),
  checkUpdates: () => invoke<UpdateReport>('check_updates'),
  diskSpace: (path?: string | null) => invoke<{ free: number; total: number }>('disk_space', { path }),
  reloadIpFilter: () => invoke<FilterStatus>('reload_ip_filter'),
  ipFilterStatus: () => invoke<FilterStatus>('ip_filter_status'),
  bridgeStatus: () => invoke<BridgeStatus>('bridge_status'),
  takePending: () => invoke<Pending[]>('take_pending'),
  removeExtension: (origin: string) => invoke<Settings>('remove_extension', { origin }),
  removeExtensionEverywhere: () => invoke<Settings>('remove_extension_everywhere'),
  approveExtension: (origin: string, allow: boolean) =>
    invoke<Settings>('approve_extension', { origin, allow }),
  clearCookies: () => invoke<number>('clear_cookies'),
  findSha256: (url: string, filename?: string | null) =>
    invoke<{ hash: string; source: string } | null>('find_sha256', { url, filename }),
  extensionDir: (firefox = false) => invoke<string>('extension_dir', { firefox }),
  browserList: () => invoke<Browser[]>('browser_list'),
  appearanceSupport: () => invoke<{ materials: string[] }>('appearance_support'),
  applyAppearance: (theme: string, transparency: boolean, material: string, tint: [number, number, number, number]) =>
    invoke<boolean>('apply_appearance', { theme, transparency, material, tint }),
  setupExtension: (browser: string) => invoke<string>('setup_extension', { browser }),
  quit: () => invoke<void>('quit_app')
};

export function errorText(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  return JSON.stringify(e);
}
