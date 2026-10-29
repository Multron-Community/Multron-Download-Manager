export type Status = 'queued' | 'active' | 'paused' | 'completed' | 'failed';
export type Kind = 'http' | 'media' | 'torrent';
export type Category = 'video' | 'music' | 'documents' | 'archives' | 'programs' | 'images' | 'other';
export type HashState = 'none' | 'pending' | 'verifying' | 'ok' | 'mismatch' | 'computed';

export type StatusFilter = 'all' | Status;
export type KindFilter = 'all' | Kind;
export type CategoryFilter = 'all' | Category;

export interface Segment {
  start: number;
  end: number;
  pos: number;
}

export interface TorrentFile {
  name: string;
  length: number;
  included: boolean;
}

export interface Download {
  id: string;
  kind: Kind;
  url: string;
  name: string;
  dir: string;
  category: Category;
  status: Status;
  total: number | null;
  downloaded: number;
  threads: number;
  resumable: boolean;
  segments: Segment[];
  createdAt: number;
  completedAt: number | null;
  error: string | null;
  referrer: string | null;
  speedLimit: number;
  queueOrder: number;
  queuePos: number | null;
  media: {
    quality: string;
    playlist: boolean;
    thumbnail: string | null;
    files: string[];
  } | null;
  torrent: {
    infoHash: string | null;
    source: string;
    torrentFile: string | null;
    onlyFiles: number[] | null;
    files: TorrentFile[];
    uploaded: number;
    trackers: string[];
    userPaused: boolean;
  } | null;
  sha256Expected: string | null;
  sha256Actual: string | null;
  hashState: HashState;
  scanState: '' | 'scanning' | 'clean' | 'threat' | 'skipped' | 'marked';
  scanDetail?: string | null;
  cloudState: '' | 'scanning' | 'clean' | 'threat' | 'suspicious' | 'skipped' | 'error' | 'locked' | 'unknown' | 'partial';
  cloudDetail?: string | null;
  speed: number;
  eta: number | null;
  connections: number;
  uploadSpeed: number;
  peers: number;
  seeding: boolean;
  phase: string | null;
  itemIndex: number | null;
  itemCount: number | null;
  toolName: string | null;
  toolDownloaded: number;
  toolTotal: number | null;
}

export interface Settings {
  language: string;
  theme: 'system' | 'dark' | 'light';
  accentColor: string;
  transparency: boolean;
  backdropMaterial: string;
  transparencyLevel: number;
  downloadDir: string;
  organizeByCategory: boolean;
  closeToTray: boolean;
  startMinimized: boolean;
  launchAtStartup: boolean;
  notifications: boolean;
  resumeOnStartup: boolean;
  checkUpdatesOnStartup: boolean;
  autoUpdate: boolean;
  threads: number;
  maxConcurrent: number;
  sequential: boolean;
  speedLimit: number;
  retries: number;
  userAgent: string;
  askSha256: boolean;
  autoDetectSha256: boolean;
  bridgeEnabled: boolean;
  bridgePort: number;
  confirmBrowserDownloads: boolean;
  ytdlpAutoUpdate: boolean;
  mediaQuality: string;
  mediaContainer: string;
  cookiesFromBrowser: string;
  embedMetadata: boolean;
  embedThumbnail: boolean;
  torrentPort: number;
  torrentDht: boolean;
  torrentUpnp: boolean;
  torrentUtp: boolean;
  torrentPeerLimit: number;
  torrentPublicTrackers: boolean;
  torrentDownloadLimit: number;
  torrentUploadLimit: number;
  seedAfterComplete: boolean;
  seedRatio: number;
  seedTimeLimit: number;
  seedLimitAction: 'pause' | 'remove' | 'delete';
  torrentMaxActiveDownloads: number;
  torrentMaxActiveSeeds: number;
  altSpeedEnabled: boolean;
  altDownloadLimit: number;
  altUploadLimit: number;
  altScheduleEnabled: boolean;
  altScheduleFrom: string;
  altScheduleTo: string;
  altScheduleDays: number[];
  watchFolderEnabled: boolean;
  watchFolder: string;
  rssEnabled: boolean;
  rssInterval: number;
  rssFeeds: RssFeed[];
  rssRules: RssRule[];
  onCompleteCommand: string;
  markDownloads: boolean;
  cloudScan: boolean;
  cloudScanButton: boolean;
  cloudScanAllButton: boolean;
  cloudScanManualLimitMb: number;
  cloudCompressUploads: boolean;
  cloudConnections: number;
  cloudFingerprintCache: boolean;
  cloudScanExtensions: string[];
  askBeforeStartingPrograms: boolean;
  fullscreenVideoButton: boolean;
  downloadNotices: boolean;
  scanNotices: boolean;
  inAppNotices: boolean;
  cloudScanAccepted: boolean;
  cloudScanLimitMb: number;
  whenDone: 'nothing' | 'sleep' | 'shutdown' | 'quit';
  extraTrackers: string[];
  torrentProxy: string;
  torrentBindInterface: string;
  torrentIpv4Only: boolean;
  torrentLsd: boolean;
  torrentDisableTrackers: boolean;
  torrentHideClient: boolean;
  torrentScrape: boolean;
  ipFilterEnabled: boolean;
  ipFilterAllowMode: boolean;
  ipFilterSource: string;
  ipFilterSourceV6: string;
  ipFilterManual: string;
  ipFilterReloadDays: number;
  torrentEnabled: boolean;
  mediaEnabled: boolean;
  tlsAcceptInvalidCerts: boolean;
  blockInsecureRedirects: boolean;
  bridgeRequirePairing: boolean;
  trustedExtensions: string[];
  extensionSetupDone: boolean;
}

export interface BrowserCookie {
  name: string;
  value: string;
  domain: string;
  path?: string;
  secure?: boolean;
  httpOnly?: boolean;
  expirationDate?: number;
}

export interface AddRequest {
  allowDuplicate?: boolean;
  url: string;
  kind?: string | null;
  dir?: string | null;
  filename?: string | null;
  threads?: number | null;
  quality?: string | null;
  container?: string | null;
  playlist?: boolean;
  referrer?: string | null;
  userAgent?: string | null;
  cookies?: BrowserCookie[];
  startPaused?: boolean;
  speedLimit?: number | null;
  sha256?: string | null;
  torrentFile?: string | null;
  onlyFiles?: number[] | null;
  pageTitle?: string | null;
  thumbnail?: string | null;
  total?: number | null;
  infoHash?: string | null;
  trackers?: string[];
}

export interface ProbeInfo {
  kind: Kind;
  url: string;
  filename: string | null;
  total: number | null;
  resumable: boolean;
  service: string | null;
  contentType: string | null;
  sha256: string | null;
  sha256Source: string | null;
  error: string | null;
}

export interface MediaInfo {
  title: string;
  thumbnail: string | null;
  duration: number | null;
  uploader: string | null;
  site: string | null;
  heights: number[];
  isPlaylist: boolean;
  entries: number | null;
  filesize: number | null;
  sizes: Record<string, { size: number; exact: boolean }>;
}

export interface TorrentPreview {
  infoHash: string;
  name: string;
  total: number;
  files: TorrentFile[];
  trackers: string[];
  private: boolean;
  pieceLength: number | null;
  comment: string | null;
}

export interface SourceRow {
  kind: string;
  details: string;
  status: string;
  peers: number | null;
  seeds: number | null;
  leechers: number | null;
  completed: number | null;
  next: number | null;
  interval: number | null;
}

export interface PeerRow {
  addr: string;
  client: string | null;
  state: string;
  connection: string | null;
  downloaded: number;
  uploaded: number;
  errors: number;
}

export interface TorrentDetails {
  info: {
    infoHash: string;
    name: string;
    total: number;
    pieceLength: number | null;
    pieces: number;
    private: boolean;
    comment: string | null;
    createdBy: string | null;
    creationDate: number | null;
    savePath: string;
    magnet: string;
    trackers: string[];
  };
  files: { index: number; name: string; length: number; included: boolean; progress: number }[];
  peers: PeerRow[];
  pieces: { count: number; have: string } | null;
  sources: SourceRow[];
  state: string;
  error: string | null;
  downloadSpeed: number;
  uploadSpeed: number;
  uploaded: number;
  progress: number;
}

export interface SessionStatus {
  running: boolean;
  downloadSpeed: number;
  uploadSpeed: number;
  peers: number;
  dhtEnabled: boolean;
  dhtNodes: number | null;
  listenPort: number | null;
  warning: string | null;
}

export interface GlobalStats {
  downloadSpeed: number;
  uploadSpeed: number;
  active: number;
  queued: number;
  torrent: SessionStatus | null;
  altSpeed: boolean;
}

export interface ToolStatus {
  ytdlpInstalled: boolean;
  ytdlpVersion: string | null;
  ytdlpPath: string;
  ffmpegAvailable: boolean;
  ffmpegPath: string | null;
  ffmpegManaged: boolean;
  ffmpegAutoInstall: boolean;
  denoAvailable: boolean;
  denoManaged: boolean;
  denoVersion: string | null;
  denoAutoInstall: boolean;
}

export interface ToolProgress {
  tool: string;
  downloaded: number;
  total: number | null;
  speed: number;
  done: boolean;
}

export interface AppUpdate {
  current: string;
  latest: string | null;
  hasUpdate: boolean;
  url: string;
  notes: string | null;
  publishedAt: string | null;
  error: string | null;
}

export interface UpdateReport {
  app: AppUpdate;
  ytdlpInstalled: string | null;
  ytdlpLatest: string | null;
  ytdlpError: string | null;
}

export interface FilterStatus {
  rules: number;
  loadedAt: number | null;
  error: string | null;
}

export interface BridgeStatus {
  running: boolean;
  port: number;
  error: string | null;
}

export type Pending =
  | { type: 'add'; request: AddRequest }
  | { type: 'duplicate'; request: AddRequest; existing: Duplicate }
  | { type: 'pair'; origin: string }
  | { type: 'openTorrent'; path: string }
  | { type: 'batch'; batch: PageBatch };

export interface Bootstrap {
  settings: Settings;
  downloads: Download[];
  version: string;
  platform: string;
  arch: string;
  dataDir: string;
  toolsDir: string;
}

export interface Browser {
  id: string;
  name: string;
  engine: 'chromium' | 'firefox';
  path: string;
  extensionsPage: string;
  flatpak?: string | null;
  flatpakAutostart?: boolean;
  flatpakGrant?: string | null;
}

export interface ReadyUpdate {
  version: string;
  path: string;
}

export interface BatchItem {
  url: string;
  kind: 'media' | 'auto' | 'torrent' | string;
  mediaType: 'video' | 'image' | 'audio' | 'file' | 'post' | string;
  name?: string | null;
  filename?: string | null;
  thumbnail?: string | null;
  referrer?: string | null;
}

export interface PageBatch {
  pageUrl: string;
  pageTitle?: string | null;
  items: BatchItem[];
  userAgent?: string | null;
  cookies?: BrowserCookie[];
}

export interface RssFeed {
  url: string;
  name: string;
  enabled: boolean;
}

export interface RssRule {
  name: string;
  enabled: boolean;
  mustContain: string;
  mustNotContain: string;
  useRegex: boolean;
  feeds: string[];
  saveDir: string;
  startPaused: boolean;
}

export interface RssItem {
  id: string;
  title: string;
  download: string;
  page?: string | null;
  published?: string | null;
  size?: number | null;
}

export interface FeedView {
  url: string;
  name: string;
  enabled: boolean;
  state: { title?: string | null; items: RssItem[]; error?: string | null; updated?: number | null };
  downloaded: string[];
}

export interface MandatoryUpdate {
  version: string;
  phase: 'downloading' | 'installing' | 'failed';
  error: string | null;
}

export interface CloudFinding {
  download: string;
  path: string;
  name: string;
  verdict: string;
  detail: string;
  route: string;
  size: number;
}

export interface CloudReport {
  id: string;
  name: string;
  running: boolean;
  stopped: boolean;
  total: number;
  done: number;
  current: string | null;
  clean: number;
  threats: number;
  suspicious: number;
  unknown: number;
  notChecked: number;
  findings: CloudFinding[];
  summary: string | null;
  state: string;
  ids: string[];
  stage: string;
  stageSent: number;
  stageTotal: number;
  startedMs: number;
  finishedMs: number;
  known: number;
  uploaded: number;
  uploadedBytes: number;
  recent: CloudFinding[];
}

export interface Duplicate {
  id: string;
  name: string;
  status: Status;
}
