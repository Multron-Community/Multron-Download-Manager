export type Status = 'queued' | 'active' | 'paused' | 'completed' | 'failed';
export type Kind = 'http' | 'media' | 'torrent';
export type Category = 'video' | 'music' | 'documents' | 'archives' | 'programs' | 'images' | 'other';
export type HashState = 'none' | 'pending' | 'verifying' | 'ok' | 'mismatch' | 'computed';

export type StatusFilter = 'all' | Status;
export type KindFilter = 'all' | Kind;
export type CategoryFilter = 'all' | Category | `custom:${string}`;

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
  cloudScannedAt?: number | null;
  quarantined?: boolean;
  typeMismatch?: boolean;
  later?: boolean;
  loginUser?: string;
  tags?: string[];
  note?: string;
  mirrors?: string[];
  signature?: string;
  extractState?: '' | 'waiting' | 'extracting' | 'done' | 'password' | 'choose' | 'failed';
  extractDetail?: string | null;
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
  accentCustom: string;
  accentContrast: 'auto' | 'exact';
  accentText: string;
  buttonColor: string;
  glassEffect: boolean;
  hoverGlow: boolean;
  clickRipple: boolean;
  stateGlow: boolean;
  motion: 'on' | 'system' | 'off';
  effectsOff: string[];
  headingColor: string;
  lightBrightness: number;
  darkBrightness: number;
  torrentDir: string;
  torrentStartPaused: boolean;
  torrentSkipEnabled: boolean;
  torrentSkipPatterns: string;
  torrentDeleteWatched: boolean;
  preventSleep: boolean;
  openFolderWhenDone: boolean;
  autoRetryMinutes: number;
  autoRetryMax: number;
  autoClearDays: number;
  minFreeGb: number;
  backgroundImage: string;
  backgroundOpacity: number;
  backgroundBlur: number;
  mediaAvailableQualities: boolean;
  clickNLoad: boolean;
  clickNLoadKnownOnly: boolean;
  checkRealType: boolean;
  askForPrograms: boolean;
  torEnabled: boolean;
  torProxy: string;
  torBlockTorrents: boolean;
  privacyNoReferrer: boolean;
  privacyClearFinishedOnExit: boolean;
  privacyClearCookiesOnExit: boolean;
  privacyForgetLinkgrabber: boolean;
  captchaOpenBrowser: boolean;
  cloudQuarantine: boolean;
  cloudHistory: boolean;
  cloudHistoryDays: number;
  cloudBlockThreats: boolean;
  cloudRescanOnOpen: boolean;
  cloudRescanDays: number;
  virustotalKey: string;
  schedulerEnabled: boolean;
  soundOnComplete: boolean;
  soundOnFail: boolean;
  schedulerStart: string;
  schedulerStopEnabled: boolean;
  schedulerStop: string;
  schedulerDays: number[];
  schedulerScope: 'later' | 'all';
  siteLogins: SiteLogin[];
  siteLimits: SiteLimit[];
  fileRules: FileRule[];
  historyEnabled: boolean;
  historyWarn: boolean;
  showStats: boolean;
  showHistory: boolean;
  showTags: boolean;
  sidebarLayout: SidebarGroup[];
  progressWindow: boolean;
  siteOptions: boolean;
  siteRules: SiteRule[];
  autoBackup: boolean;
  autoBackupCount: number;
  autoBackupUnit: 'day' | 'week' | 'month' | 'year';
  autoBackupDir: string;
  autoBackupKeep: number;
  shortcutsEnabled: boolean;
  privacyGpc: boolean;
  privacyHideNames: boolean;
  privacyClearHistoryOnExit: boolean;
  checkSignatures: boolean;
  loginsAuto: boolean;
  virustotalButton: boolean;
  captchaSound: boolean;
  captchaNotice: boolean;
  captchaAutoRetry: boolean;
  captchaRetrySecs: number;
  captchaTimeoutSecs: number;
  autoExtract: boolean;
  extractSubfolder: boolean;
  extractDeleteArchives: boolean;
  extractPasswords: string[];
  extractAskPassword: boolean;
  httpsOnly: boolean;
  blockInsecurePrograms: boolean;
  blockPrivateLinks: boolean;
  askBeforeOpeningFiles: boolean;
  skipTorrentPrograms: boolean;
  warnDisguisedNames: boolean;
  sidebarShortcuts: 'list' | 'tiles';
  roundedCorners: boolean;
  deleteFilesByDefault: boolean;
  clipboardMonitor: boolean;
  categoryFolders: Record<string, string>;
  customCategories: CustomCategory[];
  categoryOrder: string[];
  categoryFolderLanguage: string;
  categoryLanguageAsked: string;
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
  searchSites: SearchSite[];
  searchDisabled: string[];
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

export interface HistoryEntry {
  id: number;
  download: string;
  name: string;
  state: string;
  detail: string;
  files: number;
  threats: number;
  at: number;
  quarantined: boolean;
}

export interface QuarantineItem {
  id: number;
  download: string;
  name: string;
  original: string;
  stored: string;
  detail: string;
  size: number;
  at: number;
}

export interface VtResult {
  state: 'found' | 'not_found' | 'queued';
  name: string;
  sha256: string;
  link: string;
  malicious: number;
  suspicious: number;
  harmless: number;
  undetected: number;
  engines: number;
  label: string | null;
}

export interface CaptchaRequest {
  id: number;
  url: string;
  site: string;
  kind: string;
  download: string | null;
  createdAt: number;
}

export interface SiteLimit {
  host: string;
  speedKb: number;
  maxConnections: number;
  enabled: boolean;
}

export interface FileRule {
  enabled: boolean;
  field: 'host' | 'ext' | 'name';
  pattern: string;
  action: 'move' | 'rename';
  target: string;
}

export interface HistoryItem {
  id: string;
  name: string;
  url: string;
  size: number | null;
  kind: string;
  host: string;
  dir: string;
  at: number;
}

export interface SiteLogin {
  host: string;
  user: string;
  password: string;
  kind: 'basic' | 'bearer';
  subdomains: boolean;
  enabled: boolean;
}

export interface AddRequest {
  allowDuplicate?: boolean;
  later?: boolean;
  mirrors?: string[];
  tags?: string[];
  loginUser?: string | null;
  loginPassword?: string | null;
  rememberLogin?: boolean;
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
  note?: string;
  autoExtract?: boolean | null;
  extractPassword?: string | null;
}

export interface FoundLink {
  url: string;
  tag: string;
  text: string | null;
}

export interface PageScan {
  url: string;
  title: string | null;
  isFile: boolean;
  contentType: string | null;
  links: FoundLink[];
}

export interface Podcast {
  name: string;
  author: string;
  feedUrl: string;
  artwork: string | null;
  episodes: number | null;
}

export interface BackupFile {
  path: string;
  name: string;
  created: number;
  size: number;
}

export interface SiteRule {
  enabled: boolean;
  name: string;
  host: string;
  pattern: string;
  onlyRule: boolean;
  follow: boolean;
  onAdd: boolean;
}

export interface SidebarGroup {
  id: string;
  title: string;
  items: string[];
  collapsed: boolean;
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
  audioOnly?: boolean;
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

export interface CustomCategory {
  name: string;
  extensions: string[];
  folder: string;
  icon?: string;
}

export interface CategoryLanguage {
  from: string;
  to: string;
  folders: [string, string][];
}

export type Pending =
  | ({ type: 'categoryLanguage' } & CategoryLanguage)
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

export interface SearchSite {
  name: string;
  url: string;
  enabled: boolean;
}

export interface SearchSiteView {
  id: string;
  name: string;
  url: string;
  builtin: boolean;
  enabled: boolean;
}

export interface SearchResult {
  site: string;
  title: string;
  download: string;
  page?: string | null;
  size?: number | null;
  seeders?: number | null;
  leechers?: number | null;
  published?: string | null;
}

export interface SearchOutcome {
  results: SearchResult[];
  errors: { site: string; error: string }[];
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
  previous?: boolean;
  at?: number | null;
}
