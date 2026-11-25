<script lang="ts">
  import { CONTAINERS, QUALITIES } from '../lib/qualities';
  import { onMount, untrack } from 'svelte';
  import { open as openDialog, ask } from '@tauri-apps/plugin-dialog';
  import { app, doneSound } from '../lib/store.svelte';
  import { api, errorText } from '../lib/api';
  import { t, i18n, LANGUAGES, setLanguage, reloadLanguages, openLanguageFolder } from '../lib/i18n.svelte';
  import { bytes, date, duration, percent, speed } from '../lib/format';
  import type { BackupFile, SiteRule, HistoryEntry, QuarantineItem, BridgeStatus, FilterStatus, Settings, ToolStatus, UpdateReport } from '../types';
  import Icon from './Icon.svelte';
  import { ACCENTS, MATERIALS, resolveTheme } from '../lib/theme';
  import { BUILTIN_BACKGROUNDS, builtinBackground } from '../lib/backgrounds';

  interface Section {
    id: string;
    label: string;
    icon: string;
    parent?: string;
    keywords: string;
  }

  const sections: Section[] = [
    { id: 'appearance', label: 'Appearance', icon: 'palette', keywords: 'theme dark light system windows mode color accent transparency transparent mica acrylic blur vibrancy' },
    { id: 'general', label: 'Startup & Shutdown', icon: 'bolt', keywords: 'start minimized tray close notifications resume' },
    { id: 'language', label: 'Language', icon: 'globe', keywords: 'language translation locale dil sprache idioma langue lingua extension' },
    { id: 'files', label: 'Files', icon: 'folder', keywords: 'download folder directory organize category' },
    { id: 'connection', label: 'Connection', icon: 'link', keywords: 'threads connections parallel concurrent retries user agent' },
    { id: 'transfer', label: 'Transfer', icon: 'down', keywords: 'speed limit bandwidth queue' },
    { id: 'integrity', label: 'Integrity (SHA-256)', icon: 'hash', keywords: 'sha256 hash checksum verify' },
    { id: 'browser', label: 'Browser Integration', icon: 'globe', keywords: 'extension bridge port chrome firefox edge' },
    { id: 'media', label: 'Media (yt-dlp)', icon: 'film', keywords: 'youtube reddit instagram yt-dlp ffmpeg quality cookies' },
    { id: 'torrent', label: 'Torrent', icon: 'magnet', keywords: 'port listen tcp udp utp upnp dht lsd peers' },
    { id: 'trackers', label: 'Trackers', icon: 'list', parent: 'torrent', keywords: 'tracker announce scrape public' },
    { id: 'seeding', label: 'Seeding & Limits', icon: 'up', parent: 'torrent', keywords: 'seed ratio upload download limit' },
    { id: 'privacy', label: 'Privacy', icon: 'shield', parent: 'torrent', keywords: 'proxy socks5 vpn interface ipv4 client identity anonymous' },
    { id: 'ipfilters', label: 'IP Filters', icon: 'filter', parent: 'torrent', keywords: 'ip filter blocklist allow deny dat p2p peerguardian emule' },
    { id: 'queue', label: 'Queueing', icon: 'queue', parent: 'torrent', keywords: 'queue active downloads seeds limit order' },
    { id: 'watch', label: 'Watch Folder', icon: 'folderOpen', parent: 'torrent', keywords: 'watch folder auto add torrent files' },
    { id: 'rss', label: 'RSS Feeds', icon: 'rss', keywords: 'rss feed atom auto download rules filter subscribe' },
    { id: 'scheduler', label: 'Scheduler', icon: 'clock', keywords: 'scheduler queue start stop later time days night alternative speed limit turtle schedule' },
    { id: 'sitelimits', label: 'Per-site limits', icon: 'gauge', keywords: 'site host speed limit connections bandwidth priority' },
    { id: 'siterules', label: 'Site rules', icon: 'code', keywords: 'site rules plugins parser custom website link pattern regex hoster extract linkgrabber' },
    { id: 'rules', label: 'Rename & move rules', icon: 'edit', keywords: 'rules rename move sort organize pattern finished files automatic' },
    { id: 'logins', label: 'Site logins', icon: 'users', keywords: 'login password authorization user account basic token site credentials' },
    { id: 'automation', label: 'When Downloads Finish', icon: 'power', keywords: 'shutdown sleep quit command run program finish complete' },
    { id: 'components', label: 'Components', icon: 'puzzle', keywords: 'plugins components engine modules load startup' },
    { id: 'cloud', label: 'Multron Cloud', icon: 'cloud', keywords: 'cloud scan malware virus real-time realtime quarantine history threat restore block rescan virustotal' },
    { id: 'anonymity', label: 'Privacy', icon: 'eye', keywords: 'privacy tor onion anonymous proxy socks ip referrer cookies clear history forget' },
    { id: 'virustotal', label: 'VirusTotal', icon: 'plus', keywords: 'virustotal virus total api key antivirus engines hash upload' },
    { id: 'captcha', label: 'CAPTCHAs', icon: 'shield', keywords: 'captcha recaptcha hcaptcha turnstile solve human check browser sound retry' },
    { id: 'security', label: 'Security', icon: 'lock', keywords: 'tls certificate ssl pairing trusted extensions cookies' },
    { id: 'backup', label: 'Backup & restore', icon: 'archive', keywords: 'backup restore export import transfer move computer settings' },
    { id: 'about', label: 'Updates & About', icon: 'info', keywords: 'version update check about license' }
  ];

  let draft = $state<Settings | null>(null);
  let newPassword = $state('');
  let lightTheme = $derived(draft ? resolveTheme(draft.theme) === 'light' : false);
  let shownPasswords = $state<Set<number>>(new Set());

  const SECURITY_KEYS = [
    'markDownloads',
    'skipTorrentPrograms',
    'checkSignatures',
    'checkRealType',
    'warnDisguisedNames',
    'askForPrograms',
    'askBeforeStartingPrograms',
    'bridgeRequirePairing',
    'tlsAcceptInvalidCerts',
    'blockInsecureRedirects',
    'blockInsecurePrograms',
    'blockPrivateLinks',
    'clickNLoad',
    'clickNLoadKnownOnly',
    'httpsOnly',
    'askBeforeOpeningFiles',
    'autoUpdate',
    'checkUpdatesOnStartup',
    'ytdlpAutoUpdate',
    'autoDetectSha256',
    'confirmBrowserDownloads',
    'cloudBlockThreats',
    'cloudQuarantine',
    'cloudRescanOnOpen',
    'privacyClearCookiesOnExit',
    'autoExtract'
  ] as const;
  type SecurityKey = (typeof SECURITY_KEYS)[number];
  const securityBase: Record<SecurityKey, boolean> = {
    markDownloads: true,
    skipTorrentPrograms: true,
    checkSignatures: true,
    checkRealType: true,
    warnDisguisedNames: true,
    askForPrograms: false,
    askBeforeStartingPrograms: false,
    bridgeRequirePairing: true,
    tlsAcceptInvalidCerts: false,
    blockInsecureRedirects: false,
    blockInsecurePrograms: false,
    blockPrivateLinks: false,
    clickNLoad: true,
    clickNLoadKnownOnly: false,
    httpsOnly: false,
    askBeforeOpeningFiles: false,
    autoUpdate: true,
    checkUpdatesOnStartup: true,
    ytdlpAutoUpdate: true,
    autoDetectSha256: true,
    confirmBrowserDownloads: false,
    cloudBlockThreats: true,
    cloudQuarantine: true,
    cloudRescanOnOpen: true,
    privacyClearCookiesOnExit: false,
    autoExtract: true
  };
  const securityStandard = { ...securityBase, askForPrograms: true, askBeforeStartingPrograms: true, blockInsecureRedirects: true, confirmBrowserDownloads: true };
  const securityHigh = { ...securityStandard, blockInsecurePrograms: true, blockPrivateLinks: true, clickNLoadKnownOnly: true, privacyClearCookiesOnExit: true };
  const SECURITY_EXTRA: Record<string, string> = {
    standard: 'Also asks before a download from the browser starts.',
    high: 'Also asks before a download from the browser starts, and removes saved browser cookies when the app closes.',
    strict: 'Also asks before a download from the browser starts, removes saved browser cookies when the app closes and does not unpack archives automatically.'
  };
  const SECURITY_LEVELS: { id: string; label: string; desc: string; values: Record<SecurityKey, boolean> }[] = [
    { id: 'basic', label: 'Basic', desc: 'Checks run quietly in the background and nothing asks for confirmation. Disguised programs and broken signatures are still marked.', values: securityBase },
    { id: 'standard', label: 'Standard (recommended)', desc: 'The default. Asks before programs are downloaded from the browser or started, and stops secure links that redirect to unencrypted ones.', values: securityStandard },
    { id: 'high', label: 'High', desc: 'Standard, and also refuses programs from unencrypted links, refuses links to your local network from the browser and accepts Click\'n\'Load only from known link-protection sites.', values: securityHigh },
    { id: 'strict', label: 'Strict', desc: 'High, and also refuses every unencrypted (http://) link, asks before any downloaded file opens and turns Click\'n\'Load off.', values: { ...securityHigh, httpsOnly: true, askBeforeOpeningFiles: true, clickNLoad: false, autoExtract: false } }
  ];
  let securityLevel = $derived.by(() => {
    const d = draft;
    if (!d) return 'standard';
    return SECURITY_LEVELS.find((l) => SECURITY_KEYS.every((k) => d[k] === l.values[k]))?.id ?? 'custom';
  });
  let securityStep = $derived(Math.max(0, SECURITY_LEVELS.findIndex((l) => l.id === securityLevel)));
  function applySecurityLevel(id: string) {
    const level = SECURITY_LEVELS.find((l) => l.id === id);
    if (!level || !draft) return;
    for (const k of SECURITY_KEYS) draft[k] = level.values[k];
  }

  function securityKey(e: KeyboardEvent) {
    const last = SECURITY_LEVELS.length - 1;
    const current = securityLevel === 'custom' ? 1 : securityStep;
    const next =
      e.key === 'ArrowUp' || e.key === 'ArrowRight' ? current + 1
      : e.key === 'ArrowDown' || e.key === 'ArrowLeft' ? current - 1
      : e.key === 'Home' ? 0
      : e.key === 'End' ? last
      : null;
    if (next === null) return;
    e.preventDefault();
    applySecurityLevel(SECURITY_LEVELS[Math.max(0, Math.min(last, next))].id);
  }

  function securityPointer(e: PointerEvent) {
    const el = e.currentTarget as HTMLElement;
    if (e.type === 'pointerdown') el.setPointerCapture?.(e.pointerId);
    const r = el.getBoundingClientRect();
    const last = SECURITY_LEVELS.length - 1;
    const frac = (e.clientY - r.top - 10) / Math.max(1, r.height - 20);
    const index = Math.round((1 - Math.max(0, Math.min(1, frac))) * last);
    if (securityLevel !== SECURITY_LEVELS[index].id) applySecurityLevel(SECURITY_LEVELS[index].id);
  }

  function addPassword() {
    const p = newPassword.trim();
    if (!p || !draft) return;
    if (!draft.extractPasswords.includes(p)) draft.extractPasswords = [...draft.extractPasswords.filter((x) => x.trim()), p];
    newPassword = '';
  }
  let filter = $state('');
  let trackersText = $state('');
  let tools = $state<ToolStatus | null>(null);
  let updates = $state<UpdateReport | null>(null);
  let checking = $state(false);
  let updateProgress = $derived(app.toolProgress?.tool.startsWith('Multron Download Manager') ? app.toolProgress : null);
  let filterStatus = $state<FilterStatus | null>(null);
  let bridge = $state<BridgeStatus | null>(null);
  let toolBusy = $state<string | null>(null);
  let smartscreen = $state<'on' | 'off' | 'missing' | 'unsupported' | null>(null);
  const cloudExtensionsByPlatform: Record<string, { defaults: string[]; more: string[] }> = {
    windows: {
      defaults: ['exe', 'msi', 'msix', 'appx', 'dll', 'scr', 'com', 'bat', 'cmd', 'ps1', 'vbs', 'js', 'jar', 'lnk', 'hta', 'reg'],
      more: ['zip', '7z', 'rar', 'iso', 'img', 'vhd', 'cab', 'docm', 'xlsm', 'pptm', 'pdf', 'apk']
    },
    macos: {
      defaults: ['dmg', 'pkg', 'mpkg', 'command', 'tool', 'sh', 'zsh', 'py', 'jar', 'scpt', 'dylib'],
      more: ['zip', '7z', 'rar', 'xip', 'iso', 'img', 'tar', 'gz', 'docm', 'xlsm', 'pdf', 'apk']
    },
    linux: {
      defaults: ['appimage', 'deb', 'rpm', 'run', 'bin', 'sh', 'py', 'pl', 'jar', 'flatpakref', 'snap', 'desktop'],
      more: ['zip', '7z', 'rar', 'iso', 'img', 'tar', 'gz', 'xz', 'docm', 'xlsm', 'pdf', 'apk']
    }
  };
  const cloudExtensionSet = $derived(cloudExtensionsByPlatform[app.platform] ?? cloudExtensionsByPlatform.linux);
  const cloudExtensionDefaults = $derived(cloudExtensionSet.defaults);
  const cloudExtensionChoices = $derived([...cloudExtensionSet.defaults, ...cloudExtensionSet.more]);
  let customExtensions = $state('');

  function toggleExtension(ext: string, on: boolean) {
    if (!draft) return;
    const rest = draft.cloudScanExtensions.filter((x) => x !== ext);
    draft.cloudScanExtensions = on ? [...rest, ext] : rest;
  }

  async function resetAllSettings() {
    const yes = await ask(t('Reset every setting to its default? Your downloads, download history and the connection to the browser extension stay, but site logins, rules, categories, feeds and keys are removed. Save a backup first if you may want them back.'), { title: t('Reset all settings'), kind: 'warning', okLabel: t('Reset'), cancelLabel: t('Cancel') }).catch(() => false);
    if (!yes) return;
    try {
      app.settings = await api.resetSettings();
      resetDraft();
      await setLanguage(app.settings.language);
      app.toast('success', t('All settings were reset'));
    } catch (e) {
      app.toast('error', errorText(e));
    }
  }

  function resetDraft() {
    if (!app.settings) return;
    draft = JSON.parse(JSON.stringify(app.settings));
    trackersText = draft!.extraTrackers.join('\n');
  }

  $effect(() => {
    if (app.settings) untrack(resetDraft);
  });

  $effect(() => {
    if (!draft) return;
    app.appearancePreview = {
      theme: draft.theme,
      accentColor: draft.accentColor,
      accentCustom: draft.accentCustom,
      accentContrast: draft.accentContrast,
      accentText: draft.accentText,
      buttonColor: draft.buttonColor,
      glassEffect: draft.glassEffect,
      hoverGlow: draft.hoverGlow,
      clickRipple: draft.clickRipple,
      stateGlow: draft.stateGlow,
      motion: draft.motion,
      effectsOff: draft.effectsOff,
      headingColor: draft.headingColor,
      lightBrightness: draft.lightBrightness,
      darkBrightness: draft.darkBrightness,
      backgroundImage: draft.backgroundImage,
      backgroundOpacity: draft.backgroundOpacity,
      backgroundBlur: draft.backgroundBlur,
      roundedCorners: draft.roundedCorners,
      transparency: draft.transparency,
      backdropMaterial: draft.backdropMaterial,
      transparencyLevel: draft.transparencyLevel
    };
  });

  let history = $state<HistoryEntry[]>([]);
  let quarantine = $state<QuarantineItem[]>([]);
  let protectionTab = $state<'quarantine' | 'history'>('quarantine');
  let historyFilter = $state<'all' | 'threat' | 'clean' | 'other'>('all');
  let showVtKey = $state(false);
  let bgPreview = $state<string | null>(null);
  let bgPreviewFor = '';
  $effect(() => {
    const path = draft?.backgroundImage ?? '';
    if (path === bgPreviewFor) return;
    bgPreviewFor = path;
    if (!path) {
      bgPreview = null;
      return;
    }
    const builtin = builtinBackground(path);
    if (builtin) {
      bgPreview = builtin.thumb;
      return;
    }
    api.backgroundData(path).then((u) => {
      if (bgPreviewFor === path) bgPreview = u;
    }).catch(() => (bgPreview = null));
  });

  async function chooseBackground() {
    if (!draft) return;
    const res = await openDialog({ multiple: false, filters: [{ name: t('Images'), extensions: ['png', 'jpg', 'jpeg', 'webp', 'gif', 'bmp', 'avif'] }] });
    if (typeof res !== 'string') return;
    try {
      draft.backgroundImage = await api.backgroundStore(res);
    } catch (e) {
      app.toast('error', errorText(e));
    }
  }
  let torState = $state<{ busy: boolean; text: string; tone: 'ok' | 'bad' | 'muted' }>({ busy: false, text: '', tone: 'muted' });

  async function detectTor() {
    if (!draft) return;
    torState = { busy: true, text: t('Looking for Tor…'), tone: 'muted' };
    const found = await api.torDetect().catch(() => null);
    if (found) {
      draft.torProxy = found;
      torState = { busy: false, text: t('Tor found at {address}.', { address: found }), tone: 'ok' };
    } else {
      torState = { busy: false, text: t('Tor was not found. Start Tor Browser (port 9150) or the Tor service (port 9050) first.'), tone: 'bad' };
    }
  }

  async function testTor() {
    if (!draft) return;
    torState = { busy: true, text: t('Checking the connection through Tor…'), tone: 'muted' };
    try {
      const r = await api.torCheck(draft.torProxy);
      torState = r.isTor
        ? { busy: false, text: t('Connected through Tor. Sites see the address {ip}, not yours.', { ip: r.ip }), tone: 'ok' }
        : { busy: false, text: t('This proxy is not Tor. Sites see {ip}.', { ip: r.ip }), tone: 'bad' };
    } catch (e) {
      torState = { busy: false, text: errorText(e), tone: 'bad' };
    }
  }
  let shownHistory = $derived(
    history.filter((h) =>
      historyFilter === 'all'
        ? true
        : historyFilter === 'threat'
          ? h.state === 'threat' || h.state === 'suspicious'
          : historyFilter === 'clean'
            ? h.state === 'clean' || h.state === 'unknown'
            : !['threat', 'suspicious', 'clean', 'unknown'].includes(h.state)
    )
  );

  async function loadProtection() {
    history = await api.cloudHistory().catch(() => []);
    quarantine = await api.quarantineList().catch(() => []);
  }

  $effect(() => {
    if (app.settingsSection === 'protection') untrack(loadProtection);
  });

  async function restoreItem(q: QuarantineItem) {
    const yes = await ask(t('Restore {name} to its folder? Only do this if you are sure the file is safe.', { name: q.name }), { title: t('Restore from quarantine'), kind: 'warning', okLabel: t('Restore'), cancelLabel: t('Cancel') }).catch(() => false);
    if (!yes) return;
    if (await app.run(() => api.quarantineRestore(q.id), t('Restored'))) loadProtection();
  }

  async function deleteItem(q: QuarantineItem) {
    const yes = await ask(t('Delete {name} for good? This cannot be undone.', { name: q.name }), { title: t('Delete from quarantine'), kind: 'warning', okLabel: t('Delete'), cancelLabel: t('Cancel') }).catch(() => false);
    if (!yes) return;
    if (await app.run(() => api.quarantineDelete(q.id))) loadProtection();
  }

  async function clearHistory() {
    const yes = await ask(t('Clear the whole scan history?'), { title: t('Scan history'), kind: 'warning', okLabel: t('Clear'), cancelLabel: t('Cancel') }).catch(() => false);
    if (!yes) return;
    await api.cloudHistoryClear().catch(() => {});
    loadProtection();
  }

  const stateTone: Record<string, string> = { threat: 'text-bad', suspicious: 'text-warn', clean: 'text-ok', unknown: 'text-ok' };
  const stateLabel: Record<string, string> = { threat: 'Threat', suspicious: 'Suspicious', clean: 'Clean', unknown: 'Clean' };

  let extractTool = $state<boolean | null>(null);
  api.extractToolAvailable().then((v) => (extractTool = v)).catch(() => {});

  onMount(() => {
    refreshTools();
    api.ipFilterStatus().then((s) => (filterStatus = s));
    api.bridgeStatus().then((s) => (bridge = s));
    api.smartscreenStatus().then((s) => (smartscreen = s)).catch(() => (smartscreen = 'unsupported'));
    return () => (app.appearancePreview = null);
  });

  async function refreshTools() {
    try {
      tools = await api.toolsStatus();
    } catch {
      tools = null;
    }
  }

  let dirty = $derived(
    !!draft && !!app.settings &&
      (JSON.stringify({ ...draft, extraTrackers: trackersText.split('\n').map((s) => s.trim()).filter(Boolean) }) !==
        JSON.stringify(app.settings))
  );

  async function save() {
    if (!draft) return;
    const next: Settings = {
      ...draft,
      extraTrackers: trackersText.split('\n').map((s) => s.trim()).filter(Boolean),
      threads: Number(draft.threads),
      maxConcurrent: Number(draft.maxConcurrent),
      retries: Number(draft.retries),
      autoRetryMinutes: Number(draft.autoRetryMinutes) || 0,
      autoRetryMax: Number(draft.autoRetryMax) || 3,
      autoClearDays: Number(draft.autoClearDays) || 0,
      minFreeGb: Number(draft.minFreeGb) || 0,
      speedLimit: Number(draft.speedLimit) || 0,
      bridgePort: Number(draft.bridgePort),
      torrentPort: Number(draft.torrentPort),
      torrentPeerLimit: Number(draft.torrentPeerLimit),
      cloudScanLimitMb: Number(draft.cloudScanLimitMb) || 15,
      cloudScanManualLimitMb: Number(draft.cloudScanManualLimitMb) || 100,
      cloudConnections: Math.min(8, Math.max(1, Number(draft.cloudConnections) || 4)),
      cloudScanExtensions: [...new Set([...draft.cloudScanExtensions, ...customExtensions.split(/[\s,;]+/).map((x) => x.trim().replace(/^\./, '').toLowerCase()).filter(Boolean)])],
      torrentDownloadLimit: Number(draft.torrentDownloadLimit) || 0,
      torrentUploadLimit: Number(draft.torrentUploadLimit) || 0,
      seedRatio: Number(draft.seedRatio) || 0,
      seedTimeLimit: Number(draft.seedTimeLimit) || 0,
      torrentMaxActiveDownloads: Number(draft.torrentMaxActiveDownloads) || 0,
      torrentMaxActiveSeeds: Number(draft.torrentMaxActiveSeeds) || 0,
      altDownloadLimit: Number(draft.altDownloadLimit) || 0,
      altUploadLimit: Number(draft.altUploadLimit) || 0,
      rssInterval: Number(draft.rssInterval) || 30,
      ipFilterReloadDays: Number(draft.ipFilterReloadDays) || 7
    };
    if (await app.saveSettings(next)) {
      app.toast('success', 'Settings saved');
      setTimeout(() => api.bridgeStatus().then((s) => (bridge = s)), 600);
    }
  }

  let visible = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return sections;
    return sections.filter((s) => (s.label + ' ' + s.keywords).toLowerCase().includes(q));
  });

  const CATEGORY_NAMES: [string, string][] = [
    ['video', 'Video'],
    ['music', 'Music'],
    ['documents', 'Documents'],
    ['archives', 'Archives'],
    ['programs', 'Programs'],
    ['images', 'Images'],
    ['other', 'Other']
  ];
  const sep = navigator.userAgent.includes('Windows') ? '\\' : '/';

  function setCategoryFolder(key: string, value: string) {
    if (!draft) return;
    const next = { ...draft.categoryFolders };
    if (value.trim()) next[key] = value;
    else delete next[key];
    draft.categoryFolders = next;
  }

  async function browseCategory(key: string) {
    const res = await openDialog({ directory: true, defaultPath: draft?.categoryFolders[key] || draft?.downloadDir || undefined });
    if (typeof res === 'string') setCategoryFolder(key, res);
  }

  async function browseCustom(i: number) {
    const res = await openDialog({ directory: true, defaultPath: draft?.customCategories[i]?.folder || draft?.downloadDir || undefined });
    if (typeof res === 'string' && draft) draft.customCategories[i].folder = res;
  }

  async function browseDir() {
    const res = await openDialog({ directory: true, defaultPath: draft?.downloadDir || undefined });
    if (typeof res === 'string' && draft) draft.downloadDir = res;
  }

  async function browseFilter(v6: boolean) {
    const res = await openDialog({ filters: [{ name: 'IP filter', extensions: ['dat', 'p2p', 'txt', 'gz', 'zip'] }] });
    if (typeof res === 'string' && draft) {
      if (v6) draft.ipFilterSourceV6 = res;
      else draft.ipFilterSource = res;
    }
  }

  const IP_LISTS = [
    { url: 'https://github.com/Naunter/BT_BlockLists/raw/master/bt_blocklists.gz', name: 'BT BlockLists', desc: 'A combined list for BitTorrent: anti-P2P companies, bad peers and known spies. The most complete one, about 8 MB.' },
    { url: 'https://list.iblocklist.com/?list=ydxerpxkpcfqjaybcssw&fileformat=p2p&archiveformat=gz', name: 'iBlocklist Level 1', desc: 'Companies that watch file sharing (anti-P2P), from iBlocklist. About 4 MB.' },
    { url: 'https://list.iblocklist.com/?list=gyisgnzbhppbvsphucsw&fileformat=p2p&archiveformat=gz', name: 'iBlocklist Level 2', desc: 'General corporate and government ranges, from iBlocklist. Stricter: it can block some normal peers. About 1 MB.' },
    { url: 'https://www.spamhaus.org/drop/drop.txt', name: 'Spamhaus DROP', desc: 'Networks taken over by spammers and criminals. Small and safe to use.' }
  ];

  async function resetCategories() {
    if (!draft) return;
    const yes = await ask(t('Reset all categories? Your own categories are removed and the folders and order go back to the defaults. Downloaded files are not moved or deleted.'), { title: t('Reset categories'), kind: 'warning', okLabel: t('Reset'), cancelLabel: t('Cancel') }).catch(() => false);
    if (!yes) return;
    draft.customCategories = [];
    draft.categoryFolders = {};
    draft.categoryOrder = [];
    draft.sidebarLayout = (draft.sidebarLayout ?? []).map((g) => ({ ...g, items: g.items.filter((i) => !i.startsWith('cat:')) }));
    await save();
    if (app.categoryFilter !== 'all' && String(app.categoryFilter).startsWith('custom:')) app.categoryFilter = 'all';
    app.toast('success', t('Categories were reset'));
  }

  async function useIpList(url: string) {
    if (!draft) return;
    draft.ipFilterSource = url;
    draft.ipFilterEnabled = true;
    draft.ipFilterAllowMode = false;
    await loadFilterNow();
  }

  async function loadFilterNow() {
    if (dirty) await save();
    filterStatus = await api.reloadIpFilter();
    if (filterStatus.error) app.toast('error', filterStatus.error);
    else app.toast('success', t('{count} IP ranges loaded', { count: filterStatus.rules }));
  }

  async function checkUpdates() {
    checking = true;
    try {
      updates = await api.checkUpdates();
    } catch (e) {
      app.toast('error', errorText(e));
    } finally {
      checking = false;
    }
  }

  async function runTool(name: 'ytdlp' | 'ffmpeg' | 'deno') {
    toolBusy = name;
    const ok = await app.run(
      () => (name === 'ytdlp' ? api.updateYtdlp() : name === 'deno' ? api.installDeno() : api.installFfmpeg()),
      name === 'ytdlp' ? 'yt-dlp is up to date' : name === 'deno' ? 'Deno installed' : 'FFmpeg installed'
    );
    toolBusy = null;
    if (ok) {
      await refreshTools();
      if (updates) updates = { ...updates, ytdlpInstalled: tools?.ytdlpVersion ?? updates.ytdlpInstalled };
    }
  }

  async function addPublicTrackers() {
    try {
      const list = await api.publicTrackers();
      const set = new Set(trackersText.split('\n').map((s) => s.trim()).filter(Boolean));
      list.forEach((x) => set.add(x));
      trackersText = [...set].join('\n');
      app.toast('success', t('{count} public trackers added — press Save', { count: list.length }));
    } catch (e) {
      app.toast('error', errorText(e));
    }
  }

  const DAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];

  function toggleSchedulerDay(d: number) {
    if (!draft) return;
    const set = new Set(draft.schedulerDays);
    if (set.has(d)) set.delete(d);
    else set.add(d);
    draft.schedulerDays = [...set].sort();
  }

  let newLimit = $state({ host: '', speedKb: 0, maxConnections: 0 });
  function addLimit() {
    if (!draft || !newLimit.host.trim()) return;
    draft.siteLimits = [...draft.siteLimits, { host: newLimit.host.trim(), speedKb: Number(newLimit.speedKb) || 0, maxConnections: Number(newLimit.maxConnections) || 0, enabled: true }];
    newLimit = { host: '', speedKb: 0, maxConnections: 0 };
  }

  function addFileRule() {
    if (!draft) return;
    draft.fileRules = [...draft.fileRules, { enabled: true, field: 'ext', pattern: '', action: 'move', target: '' }];
  }

  function moveRule(i: number, by: number) {
    if (!draft) return;
    const j = i + by;
    if (j < 0 || j >= draft.fileRules.length) return;
    const list = [...draft.fileRules];
    [list[i], list[j]] = [list[j], list[i]];
    draft.fileRules = list;
  }

  async function browseRule(i: number) {
    const r = await openDialog({ directory: true, defaultPath: draft?.fileRules[i]?.target || draft?.downloadDir || undefined });
    if (typeof r === 'string' && draft) draft.fileRules[i].target = r;
  }

  let backupSecrets = $state(false);
  let backupBusy = $state(false);

  async function exportBackup() {
    const { save: saveDialog } = await import('@tauri-apps/plugin-dialog');
    const path = await saveDialog({ defaultPath: `multron-backup-${new Date().toISOString().slice(0, 10)}.json`, filters: [{ name: 'JSON', extensions: ['json'] }] });
    if (!path) return;
    backupBusy = true;
    await app.run(() => api.backupExport(path, backupSecrets), t('Backup saved'));
    backupBusy = false;
  }

  async function importBackup() {
    const path = await openDialog({ multiple: false, filters: [{ name: 'JSON', extensions: ['json'] }] });
    if (typeof path !== 'string') return;
    const yes = await ask(t('Restore this backup? Your current settings are replaced by the ones in the file, and downloads from the file that are not in the list yet are added.'), { title: t('Restore backup'), kind: 'warning', okLabel: t('Restore'), cancelLabel: t('Cancel') }).catch(() => false);
    if (!yes) return;
    backupBusy = true;
    try {
      const [added, settings] = await api.backupImport(path);
      app.settings = settings;
      resetDraft();
      app.toast('success', t('Backup restored. {count} downloads were added to the list.', { count: added }));
    } catch (e) {
      app.toast('error', errorText(e));
    } finally {
      backupBusy = false;
    }
  }

  let newLogin = $state({ host: '', user: '', password: '', kind: 'basic' as 'basic' | 'bearer' });
  let showPasswords = $state(false);

  function addLogin() {
    if (!draft || !newLogin.host.trim()) return;
    draft.siteLogins = [...draft.siteLogins, { host: newLogin.host.trim(), user: newLogin.user.trim(), password: newLogin.password, kind: newLogin.kind, subdomains: true, enabled: true }];
    newLogin = { host: '', user: '', password: '', kind: 'basic' };
  }

  function toggleDay(d: number) {
    if (!draft) return;
    const set = new Set(draft.altScheduleDays);
    if (set.has(d)) set.delete(d);
    else set.add(d);
    draft.altScheduleDays = [...set].sort();
  }

  let newFeed = $state('');

  function addFeed() {
    const url = newFeed.trim();
    if (!draft || !/^https?:\/\//i.test(url) || draft.rssFeeds.some((f) => f.url === url)) return;
    draft.rssFeeds = [...draft.rssFeeds, { url, name: '', enabled: true }];
    newFeed = '';
  }

  function addRule() {
    if (!draft) return;
    draft.rssRules = [
      ...draft.rssRules,
      { name: t('New rule'), enabled: true, mustContain: '', mustNotContain: '', useRegex: false, feeds: [], saveDir: '', startPaused: false }
    ];
  }

  function toggleRuleFeed(i: number, url: string) {
    if (!draft) return;
    const rule = draft.rssRules[i];
    const all = draft.rssFeeds.map((f) => f.url);
    const current = rule.feeds.length ? rule.feeds : all;
    const next = current.includes(url) ? current.filter((f) => f !== url) : [...current, url];
    rule.feeds = next.length === all.length ? [] : next;
  }

  async function browseInto(set: (path: string) => void, current?: string) {
    const res = await openDialog({ directory: true, defaultPath: current || undefined });
    if (typeof res === 'string') set(res);
  }

  function extensionBrowser(origin: string) {
    if (origin.startsWith('moz-extension://')) return 'Firefox';
    if (origin.startsWith('safari-web-extension://')) return 'Safari';
    return 'Chrome / Edge / Brave / Opera / Vivaldi';
  }

  async function removeExtension(origin: string) {
    const name = extensionBrowser(origin);
    const sure = await ask(t('Remove the Multron Download Manager extension from {name}? It removes itself within half a minute while the browser is open.', { name }), {
      title: t('Remove from browser'),
      kind: 'warning'
    });
    if (!sure) return;
    try {
      app.settings = await api.removeExtension(origin);
      app.toast('success', t('The extension removes itself from {name} within half a minute while the browser is open.', { name }));
    } catch (e) {
      app.toast('error', errorText(e));
    }
  }

  let installedBrowsers = $state<import('../types').Browser[] | null>(null);

  $effect(() => {
    if (current.id === 'browser' && installedBrowsers === null) {
      api.browserList().then((l) => (installedBrowsers = l)).catch(() => (installedBrowsers = []));
    }
  });

  function setupFor(id: string) {
    app.extensionSetupBrowser = id;
    app.extensionSetupOpen = true;
  }

  async function removeEverywhere() {
    const sure = await ask(t('Remove the Multron Download Manager extension from every browser and forget all connections? Multron Download Manager also stops registering itself with your browsers. You can install it again at any time.'), {
      title: t('Remove completely'),
      kind: 'warning'
    });
    if (!sure) return;
    try {
      app.settings = await api.removeExtensionEverywhere();
      if (draft) {
        draft.trustedExtensions = [];
        draft.extensionSetupDone = false;
      }
      app.toast('success', t('The extension removes itself from every open browser within half a minute.'));
    } catch (e) {
      app.toast('error', errorText(e));
    }
  }

  async function revoke(origin: string) {
    try {
      app.settings = await api.approveExtension(origin, false);
    } catch (e) {
      app.toast('error', errorText(e));
    }
  }

  const browsers = ['', 'chrome', 'firefox', 'edge', 'brave', 'opera', 'vivaldi', 'chromium', 'safari'];
  let current = $derived(sections.find((s) => s.id === app.settingsSection) ?? sections[0]);

  let ruleTest = $state<{ index: number; url: string; busy: boolean; result: string[] | null; error: string | null } | null>(null);
  const blankRule = (): SiteRule => ({ enabled: true, name: '', host: '', pattern: '', onlyRule: true, follow: false, onAdd: false });

  async function testRule(i: number) {
    if (!draft || !ruleTest) return;
    ruleTest = { ...ruleTest, busy: true, result: null, error: null };
    try {
      const scan = await api.siteRuleTest(draft.siteRules[i], ruleTest.url);
      ruleTest = { ...ruleTest, busy: false, result: scan.isFile ? [ruleTest.url] : scan.links.map((l) => l.url) };
    } catch (e) {
      ruleTest = { ...ruleTest, busy: false, error: errorText(e) };
    }
  }

  async function copyRules() {
    if (!draft) return;
    await navigator.clipboard.writeText(JSON.stringify({ multronSiteRules: 1, rules: draft.siteRules }, null, 2)).catch(() => {});
    app.toast('success', t('Copied to the clipboard'));
  }

  async function pasteRules() {
    if (!draft) return;
    try {
      const data = JSON.parse(await navigator.clipboard.readText());
      const list: unknown[] = Array.isArray(data) ? data : Array.isArray(data?.rules) ? data.rules : [data];
      const rules = list
        .filter((r): r is Record<string, unknown> => !!r && typeof r === 'object' && typeof (r as Record<string, unknown>).host === 'string' && typeof (r as Record<string, unknown>).pattern === 'string')
        .map((r) => ({ ...blankRule(), name: String(r.name ?? ''), host: String(r.host), pattern: String(r.pattern), onlyRule: r.onlyRule !== false, follow: r.follow === true, onAdd: r.onAdd === true }));
      if (!rules.length) throw new Error('none');
      draft.siteRules = [...draft.siteRules, ...rules];
      app.toast('success', t('{count} rules added', { count: rules.length }));
    } catch {
      app.toast('error', t('The clipboard has no site rules. Copy rules shared as JSON first.'));
    }
  }

  const EFFECTS: [string, string, string][] = [
    ['aurora', 'Moving background light', 'A soft light in the accent color slowly moves behind the window and gets livelier while something downloads.'],
    ['spotlight', 'Light that follows the mouse', 'A gentle glow in the background follows the mouse pointer.'],
    ['cards', 'Download card motion', 'Downloads slide in, lift on hover, flash green when they finish and shake when they fail.'],
    ['state', 'Moving state colors', 'The state colors on downloads breathe and a light sweeps across them. Works with Color downloads by their state.'],
    ['progress', 'Shine on progress bars', 'A light runs along the progress bar while a download is running.'],
    ['fill', 'Filling progress on cards', 'The card of a running download slowly fills with the accent color as it progresses, with a softly flowing surface.'],
    ['live', 'Live indicators', 'The pulsing dot next to Active, the glowing speed badge, the ring and bobbing icon on active downloads and numbers that pop when they change.'],
    ['accents', 'Breathing accents', 'The selected row breathes, a shine passes over New Task and titles slowly shimmer.'],
    ['transitions', 'Opening animations', 'Dialogs, menus, notices, pages and the details pane open with a short animation.']
  ];

  function allEffects(on: boolean) {
    if (!draft) return;
    draft.motion = on ? 'on' : 'off';
    draft.effectsOff = on ? [] : EFFECTS.map(([id]) => id);
    draft.hoverGlow = on;
    draft.clickRipple = on;
    draft.glassEffect = on;
    draft.stateGlow = on;
  }

  function setEffect(id: string, on: boolean) {
    if (!draft) return;
    const rest = (draft.effectsOff ?? []).filter((x) => x !== id);
    draft.effectsOff = on ? rest : [...rest, id];
  }

  let backupDir = $state('');
  let backupFiles = $state<BackupFile[]>([]);
  let backingUp = $state(false);
  const BACKUP_PRESETS: { id: string; label: string; count: number; unit: Settings['autoBackupUnit'] }[] = [
    { id: 'week', label: 'Every week', count: 1, unit: 'week' },
    { id: 'month', label: 'Every month', count: 1, unit: 'month' },
    { id: 'month3', label: 'Every 3 months', count: 3, unit: 'month' },
    { id: 'month6', label: 'Every 6 months', count: 6, unit: 'month' }
  ];
  let backupPreset = $derived(draft ? (BACKUP_PRESETS.find((p) => p.count === draft!.autoBackupCount && p.unit === draft!.autoBackupUnit)?.id ?? 'custom') : 'custom');
  let customBackup = $state(false);

  async function loadBackups() {
    try {
      [backupDir, backupFiles] = await api.autoBackups();
    } catch {
      backupFiles = [];
    }
  }

  $effect(() => {
    if (current.id === 'backup') loadBackups();
  });

  async function backupNow() {
    backingUp = true;
    try {
      await api.autoBackupNow();
      app.toast('success', t('Backup saved'));
      await loadBackups();
    } catch (e) {
      app.toast('error', errorText(e));
    } finally {
      backingUp = false;
    }
  }

  async function browseBackupDir() {
    const res = await openDialog({ directory: true, defaultPath: draft?.autoBackupDir || backupDir || undefined });
    if (typeof res === 'string' && draft) draft.autoBackupDir = res;
  }

  let startupLabel = $derived(
    app.platform === 'windows' ? t('Start with Windows') : app.platform === 'macos' ? t('Open at login') : t('Start when I log in')
  );
  let startupHint = $derived(
    app.platform === 'windows'
      ? t('Adds Multron Download Manager to the Windows startup apps (Settings › Apps › Startup). It starts hidden in the tray so browser downloads always work.')
      : app.platform === 'macos'
        ? t('Adds Multron Download Manager to Login Items (System Settings › General › Login Items). It starts hidden in the menu bar so browser downloads always work.')
        : t('Adds an autostart entry (~/.config/autostart) that GNOME, KDE, Xfce and other desktops run at login. Multron Download Manager starts hidden in the tray.')
  );
</script>

{#snippet toggle(label: string, desc: string, value: boolean, set: (v: boolean) => void)}
  <label class="settings-row flex items-start gap-3 py-[8px] px-2.5 -mx-2.5 rounded-[9px] cursor-pointer">
    <input type="checkbox" class="toggle mt-[1px]" checked={value} onchange={(e) => set(e.currentTarget.checked)} />
    <span class="flex flex-col gap-[2px]">
      <span class="text-[12.5px]">{t(label)}</span>
      {#if desc}<span class="text-[11px] text-muted leading-snug">{t(desc)}</span>{/if}
    </span>
  </label>
{/snippet}

{#snippet row(label: string, desc: string = '')}
  <div class="flex flex-col gap-[2px] pt-[6px]">
    <span class="text-[12.5px]">{t(label)}</span>
    {#if desc}<span class="text-[11px] text-muted leading-snug">{t(desc)}</span>{/if}
  </div>
{/snippet}

{#snippet group(title: string)}
  <h4 class="settings-group text-[12px] font-semibold text-white/90 mt-6 mb-1.5 pb-1.5 flex items-center gap-2"><span class="settings-group-dot"></span>{t(title)}</h4>
{/snippet}

<div class="flex-1 flex min-h-0 bg-bg">
  <aside class="w-[240px] border-r border-line bg-panel flex flex-col min-h-0">
    <div class="px-4 pt-4 pb-2 text-[14px] font-semibold flex items-center gap-2"><Icon name="settings" size={15} />{t('Options')}</div>
    <nav class="flex-1 overflow-y-auto px-2 space-y-[1px]">
      {#each visible as s}
        <button
          class="glow-target w-full h-[30px] rounded-[7px] flex items-center gap-2 text-[12.5px] cursor-pointer transition-colors {s.parent ? 'pl-[30px]' : 'px-[10px]'} {app.settingsSection === s.id ? 'bg-line text-white' : 'text-muted hover:bg-hover hover:text-white'}"
          onclick={() => (app.settingsSection = s.id)}
        >
          <Icon name={s.icon} size={14} />{t(s.label)}
        </button>
      {/each}
    </nav>
    <div class="p-3 border-t border-line">
      <div class="h-[30px] px-[10px] bg-bg border border-line rounded-full flex items-center gap-2">
        <Icon name="search" size={12} class="text-muted" />
        <input class="bg-transparent outline-none text-[12px] w-full" placeholder={t('type filter text')} bind:value={filter} />
      </div>
    </div>
  </aside>

  {#if draft}
    <div class="flex-1 flex flex-col min-h-0">
      <div class="h-[54px] px-6 flex items-center justify-between border-b border-line flex-shrink-0">
        <span class="text-[17px] font-semibold tracking-tight flex items-center gap-2.5"><span class="settings-title-icon w-[30px] h-[30px] rounded-[9px] flex items-center justify-center flex-shrink-0"><Icon name={current.icon} size={15} /></span><span class="title-glow">{t(current.label)}</span></span>
        {#if dirty}<span class="pill bg-warn/15 text-warn">{t('Unsaved changes')}</span>{/if}
      </div>

      <div class="flex-1 overflow-y-auto px-6 py-4 min-h-0">
        <div class="max-w-[760px]">
          {#if current.id === 'appearance'}
            {@render row(t('Theme'))}
            <div class="flex gap-2 mt-1 flex-wrap">
              {#each [{ id: 'system', label: 'Match the system theme', icon: 'app' }, { id: 'dark', label: 'Dark', icon: 'moon' }, { id: 'light', label: 'Light', icon: 'sun' }] as opt}
                <button
                  type="button"
                  class="h-[34px] px-3 rounded-[8px] border text-[12.5px] flex items-center gap-2 cursor-pointer transition-colors {draft.theme === opt.id ? 'border-accent/60 bg-raised text-white' : 'border-line text-muted hover:bg-hover hover:text-white'}"
                  onclick={() => (draft!.theme = opt.id as Settings['theme'])}
                >
                  <Icon name={opt.icon} size={14} />{t(opt.label)}
                </button>
              {/each}
            </div>
            <p class="text-[11px] text-muted mt-1.5 leading-snug">
              {t('“Match the system theme” is dark when Windows, macOS or Linux uses dark mode and light otherwise, and changes along with the system. The sun/moon button at the top switches manually.')}
            </p>

            {@render group('Brightness')}
            <p class="text-[11px] text-muted leading-snug mb-1.5">{t('Makes the backgrounds of each theme darker or lighter, for example when the light theme is too bright for your screen. Text keeps its contrast. 0 is the default.')}</p>
            {#each [{ key: 'lightBrightness', label: 'Light', icon: 'sun' }, { key: 'darkBrightness', label: 'Dark', icon: 'moon' }] as b (b.key)}
              {@const value = draft[b.key as 'lightBrightness' | 'darkBrightness'] ?? 0}
              <div class="flex items-center gap-3 py-1.5">
                <span class="w-[90px] flex items-center gap-2 text-[12.5px]"><Icon name={b.icon} size={13} />{t(b.label)}</span>
                <input type="range" min="-30" max="30" step="1" class="w-[260px] accent-[var(--color-accent-strong)]" value={value} oninput={(e) => (draft![b.key as 'lightBrightness' | 'darkBrightness'] = Number(e.currentTarget.value))} aria-label={t(b.label)} />
                <span class="w-[40px] text-[12px] tabular-nums {value === 0 ? 'text-muted' : 'text-white'}">{value > 0 ? '+' : ''}{value}</span>
                <button type="button" class="btn !h-[26px] !text-[11.5px]" disabled={value === 0} onclick={() => (draft![b.key as 'lightBrightness' | 'darkBrightness'] = 0)}>{t('Reset')}</button>
              </div>
            {/each}

            {@render group(t('Color'))}
            <div class="flex gap-2 flex-wrap mt-1">
              {#each ACCENTS as a}
                <button
                  type="button"
                  title={t(a.name)}
                  class="h-[34px] pl-2 pr-3 rounded-[8px] border text-[12px] flex items-center gap-2 cursor-pointer transition-colors {draft.accentColor === a.id ? 'border-accent/70 bg-raised text-white' : 'border-line text-muted hover:bg-hover hover:text-white'}"
                  onclick={() => (draft!.accentColor = a.id)}
                >
                  <span class="w-[16px] h-[16px] rounded-full flex items-center justify-center" style="background: {a.color}">
                    {#if draft.accentColor === a.id}<Icon name="check" size={10} stroke={3} class="text-white" />{/if}
                  </span>
                  {t(a.name)}
                </button>
              {/each}
              <label
                title={t('Custom color')}
                class="h-[34px] pl-2 pr-3 rounded-[8px] border text-[12px] flex items-center gap-2 cursor-pointer transition-colors {draft.accentColor === 'custom' ? 'border-accent/70 bg-raised text-white' : 'border-line text-muted hover:bg-hover hover:text-white'}"
              >
                <span class="relative w-[16px] h-[16px] rounded-full overflow-hidden flex items-center justify-center" style="background: {draft.accentCustom}">
                  {#if draft.accentColor === 'custom'}<Icon name="check" size={10} stroke={3} class="text-white relative z-[1]" />{/if}
                  <input type="color" class="absolute inset-0 opacity-0 cursor-pointer" value={draft.accentCustom} oninput={(e) => { draft!.accentCustom = e.currentTarget.value; draft!.accentColor = 'custom'; }} onclick={() => (draft!.accentColor = 'custom')} />
                </span>
                {t('Custom color')}
              </label>
            </div>
            <p class="text-[11px] text-muted mt-1.5">{t('Used for buttons, progress bars and the tint of transparent backgrounds.')}</p>
            {@render row('Readable colors', 'Automatic lightens or darkens a very dark or very light color just enough that buttons, links and outlines stay visible on the background. Exact uses the color as it is.')}
            <div class="flex flex-wrap gap-1.5 mt-2">
              {#each [['auto', 'Automatic'], ['exact', 'Exact color']] as [id, label] (id)}
                <button type="button" class="h-[30px] px-3 rounded-[8px] border text-[12px] cursor-pointer {(draft.accentContrast ?? 'auto') === id ? 'border-accent/60 bg-raised text-white' : 'border-line text-muted hover:bg-hover hover:text-white'}" onclick={() => (draft!.accentContrast = id as Settings['accentContrast'])}>{t(label)}</button>
              {/each}
            </div>
            {@render row('Button color', 'The color of all buttons, such as Scan for malware, VirusTotal, the round buttons and New Task. By default it follows the color above.')}
            <div class="flex flex-wrap items-center gap-1.5 mt-2">
              <button type="button" class="h-[30px] px-3 rounded-[8px] border text-[12px] cursor-pointer {!draft.buttonColor ? 'border-accent/60 bg-raised text-white' : 'border-line text-muted hover:bg-hover hover:text-white'}" onclick={() => (draft!.buttonColor = '')}>{t('Same as the color')}</button>
              {#each ['#2f8cf0', '#14b8a6', '#8e5ce6', '#e84a9b', '#ff8c1a', '#2eb85c', '#ffffff', '#808080'] as c (c)}
                <button type="button" class="w-[26px] h-[26px] rounded-full cursor-pointer transition-transform hover:scale-110 {draft.buttonColor === c ? 'ring-2 ring-offset-2 ring-offset-bg ring-white/80' : ''}" style="background: {c}; box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.25)" title={c} aria-label={c} onclick={() => (draft!.buttonColor = c)}></button>
              {/each}
              <label class="h-[30px] px-3 rounded-[8px] border text-[12px] flex items-center gap-2 cursor-pointer {draft.buttonColor && !['#2f8cf0', '#14b8a6', '#8e5ce6', '#e84a9b', '#ff8c1a', '#2eb85c', '#ffffff', '#808080'].includes(draft.buttonColor) ? 'border-accent/60 bg-raised text-white' : 'border-line text-muted hover:bg-hover hover:text-white'}">
                <input type="color" class="w-[16px] h-[16px] p-0 border-0 bg-transparent cursor-pointer" value={draft.buttonColor || '#2f8cf0'} oninput={(e) => (draft!.buttonColor = e.currentTarget.value)} />{t('Custom color')}
              </label>
            </div>
            {@render row('Text on buttons', 'Automatic keeps the text readable on every button. Pick a color to use it on all buttons.')}
            <div class="flex flex-wrap items-center gap-1.5 mt-2">
              {#each [['', 'Automatic'], ['#ffffff', 'White'], ['#0b0f14', 'Dark']] as [id, label] (id)}
                <button type="button" class="h-[30px] px-3 rounded-[8px] border text-[12px] cursor-pointer {(draft.accentText ?? '') === id ? 'border-accent/60 bg-raised text-white' : 'border-line text-muted hover:bg-hover hover:text-white'}" onclick={() => (draft!.accentText = id)}>{t(label)}</button>
              {/each}
              <label class="h-[30px] px-3 rounded-[8px] border text-[12px] flex items-center gap-2 cursor-pointer {draft.accentText && !['#ffffff', '#0b0f14'].includes(draft.accentText) ? 'border-accent/60 bg-raised text-white' : 'border-line text-muted hover:bg-hover hover:text-white'}">
                <input type="color" class="w-[16px] h-[16px] p-0 border-0 bg-transparent cursor-pointer" value={draft.accentText || '#ffffff'} oninput={(e) => (draft!.accentText = e.currentTarget.value)} />{t('Custom color')}
              </label>
            </div>
            <div class="flex flex-wrap items-center gap-2 mt-3 p-3 rounded-[10px] border border-line">
              <span class="text-[11px] text-dim mr-1">{t('Preview')}</span>
              <button type="button" class="btn">{t('Browse')}</button>
              <button type="button" class="chip-btn chip-accent">{t('Scan for malware')}</button>
              <button type="button" class="chip-btn">VirusTotal</button>
              <button type="button" class="round-btn" aria-label={t('Open')}><Icon name="file" size={13} /></button>
              <button type="button" class="btn btn-accent">{t('Add feed')}</button>
              <button type="button" class="btn btn-primary">{t('New Task')}</button>
            </div>

            {@render group(t('Glass effect'))}
            {@render toggle(t('Glass effect'), t('On by default. Cards, buttons and fields get a soft sheen and a light top edge, like Multron Win Cleaner.'), draft.glassEffect, (v) => (draft!.glassEffect = v))}
            <div class="pl-[42px] {draft.glassEffect ? '' : 'opacity-45 pointer-events-none'}">
              {@render toggle(t('Glow under the mouse'), t('On by default. Buttons, cards and fields light up softly where the mouse is. Works together with the glass effect.'), draft.hoverGlow, (v) => (draft!.hoverGlow = v))}
            </div>
            {@render row('Animations', 'Moving effects such as the click ripple, the light on downloads and the soft moving background. Follow Windows turns them off when Windows is set to show fewer animations.')}
            <div class="flex flex-wrap gap-1.5 mt-2 mb-2">
              {#each [['on', 'Always on'], ['system', 'Follow Windows'], ['off', 'Off']] as [id, label] (id)}
                <button type="button" class="h-[30px] px-3 rounded-[8px] border text-[12px] cursor-pointer {draft.motion === id ? 'border-accent/60 bg-raised text-white' : 'border-line text-muted hover:bg-hover hover:text-white'}" onclick={() => (draft!.motion = id as Settings['motion'])}>{t(label)}</button>
              {/each}
            </div>
            <div class="flex items-center gap-2 mb-2">
              <button type="button" class="btn !h-[28px]" onclick={() => allEffects(false)}><Icon name="x" size={12} />{t('Turn all effects off')}</button>
              <button type="button" class="btn !h-[28px]" onclick={() => allEffects(true)}><Icon name="check" size={12} />{t('Turn all effects on')}</button>
              <span class="text-[11px] text-dim">{t('Off gives a plain, still look: no animations, glow, glass sheen, ripple or state colors.')}</span>
            </div>
            <div class="pl-[42px] mb-2 {draft.motion === 'off' ? 'opacity-45 pointer-events-none' : ''}">
              {#each EFFECTS as [id, label, desc] (id)}
                {@render toggle(label, desc, !(draft.effectsOff ?? []).includes(id), (v) => setEffect(id, v))}
              {/each}
            </div>
            {@render row('Sidebar title color', 'The color of the group titles in the sidebar, such as Sources and Categories.')}
            <div class="flex flex-wrap items-center gap-1.5 mt-2 mb-2">
              {#each [['accent', 'Accent color'], ['gray', 'Gray'], ['white', lightTheme ? 'Black' : 'White']] as [id, label] (id)}
                <button type="button" class="h-[30px] px-3 rounded-[8px] border text-[12px] flex items-center gap-2 cursor-pointer {draft.headingColor === id ? 'border-accent/60 bg-raised text-white' : 'border-line text-muted hover:bg-hover hover:text-white'}" onclick={() => (draft!.headingColor = id)}>
                  <span class="w-[10px] h-[10px] rounded-full heading-swatch-{id}"></span>{t(label)}
                </button>
              {/each}
              <label class="h-[30px] px-3 rounded-[8px] border text-[12px] flex items-center gap-2 cursor-pointer {draft.headingColor.startsWith('#') ? 'border-accent/60 bg-raised text-white' : 'border-line text-muted hover:bg-hover hover:text-white'}">
                <input type="color" class="w-[16px] h-[16px] p-0 border-0 bg-transparent cursor-pointer" value={draft.headingColor.startsWith('#') ? draft.headingColor : '#8ab4ff'} oninput={(e) => (draft!.headingColor = e.currentTarget.value)} />{t('Custom color')}
              </label>
            </div>
            {@render toggle(t('Color downloads by their state'), t('Off by default. Each download gets a faint glow in the color of its state: red when it failed or a threat was found, green when it finished, the accent color while it downloads and amber when it is paused.'), draft.stateGlow, (v) => (draft!.stateGlow = v))}
            {@render toggle(t('Ripple when clicking'), t('On by default. A soft wave in the accent color spreads from where you click: inside buttons, downloads, fields and the sidebar, and as a ring on empty space.'), draft.clickRipple, (v) => (draft!.clickRipple = v))}
            {@render toggle(t('Rounded corners'), t('On by default. The sidebar, the main area and the status bar become separate cards with soft rounded corners, like Multron Win Cleaner.'), draft.roundedCorners, (v) => (draft!.roundedCorners = v))}

            {@render group('Background image')}
            <div class="flex items-start gap-4">
              <div class="w-[160px] h-[90px] rounded-[10px] border border-line bg-bg overflow-hidden flex items-center justify-center flex-shrink-0 bg-cover bg-center" style={bgPreview ? `background-image: url(${bgPreview})` : ''}>
                {#if !bgPreview}<Icon name="image" size={22} class="text-dim" />{/if}
              </div>
              <div class="flex-1 space-y-2">
                <p class="text-[11.5px] text-muted leading-snug">{t('Show a picture behind the window. The panels become see-through so the picture shines through.')}</p>
                <div class="flex gap-2">
                  <button type="button" class="btn" onclick={chooseBackground}><Icon name="image" size={13} />{draft.backgroundImage ? t('Change image') : t('Choose image')}</button>
                  {#if draft.backgroundImage}<button type="button" class="btn hover:!text-bad" onclick={() => (draft!.backgroundImage = '')}><Icon name="x" size={12} />{t('Remove')}</button>{/if}
                </div>
              </div>
            </div>
            {@const chosenBuiltin = builtinBackground(draft.backgroundImage)}
            <div class="mt-3">
              <span class="label">{t('Built-in backgrounds')}</span>
              <div class="flex flex-wrap gap-2 mt-1.5">
                {#each BUILTIN_BACKGROUNDS as b (b.id)}
                  <button
                    type="button"
                    class="builtin-bg w-[112px] h-[63px] rounded-[9px] bg-cover bg-center cursor-pointer {draft.backgroundImage === b.value ? 'is-on' : ''}"
                    style="background-image: url({b.thumb})"
                    title={t('Photo by {name} on Unsplash', { name: b.author })}
                    aria-label={t('Photo by {name} on Unsplash', { name: b.author })}
                    aria-pressed={draft.backgroundImage === b.value}
                    onclick={() => (draft!.backgroundImage = b.value)}
                  ></button>
                {/each}
              </div>
              {#if chosenBuiltin}
                <p class="text-[11px] text-muted mt-1.5">
                  {t('Photo by {name} on Unsplash', { name: chosenBuiltin.author })}
                  · <button type="button" class="text-accent hover:underline cursor-pointer" onclick={() => app.run(() => api.openUrl(chosenBuiltin.page))}>unsplash.com</button>
                </p>
              {/if}
            </div>
            <div class="grid grid-cols-[180px_minmax(0,1fr)_48px] gap-x-3 gap-y-2 items-center max-w-[560px] mt-3 {draft.backgroundImage ? '' : 'opacity-45 pointer-events-none'}">
              <span class="text-[12.5px]">{t('Panel opacity')}</span>
              <input type="range" min="20" max="98" bind:value={draft.backgroundOpacity} class="accent-[var(--color-accent-strong)]" />
              <span class="text-[11.5px] text-muted text-right">{draft.backgroundOpacity}%</span>
              <span class="text-[12.5px]">{t('Blur the image')}</span>
              <input type="range" min="0" max="30" bind:value={draft.backgroundBlur} class="accent-[var(--color-accent-strong)]" />
              <span class="text-[11.5px] text-muted text-right">{draft.backgroundBlur}px</span>
            </div>

            {@render group(t('Sidebar'))}
            {@render toggle('Show Statistics', 'A page with daily and monthly totals, the most used sites and the space each category takes.', draft.showStats, (v) => (draft!.showStats = v))}
            {@render toggle('Show Download history', 'Everything you downloaded, also after it was removed from the list.', draft.showHistory, (v) => (draft!.showHistory = v))}
            {@render toggle('Show Tags', 'Your tags as filters in the sidebar.', draft.showTags, (v) => (draft!.showTags = v))}
            {@render row(t('Search, RSS and Create torrent'), t('Show them as rows like the rest of the sidebar or as three tiles.'))}
            <div class="flex gap-2 mt-1 flex-wrap">
              {#each [{ id: 'list', label: 'Rows', icon: 'list' }, { id: 'tiles', label: 'Tiles', icon: 'app' }] as opt}
                <button
                  type="button"
                  class="h-[34px] px-3 rounded-[8px] border text-[12.5px] flex items-center gap-2 cursor-pointer transition-colors {draft.sidebarShortcuts === opt.id ? 'border-accent/60 bg-raised text-white' : 'border-line text-muted hover:bg-hover hover:text-white'}"
                  onclick={() => (draft!.sidebarShortcuts = opt.id as Settings['sidebarShortcuts'])}
                >
                  <Icon name={opt.icon} size={14} />{t(opt.label)}
                </button>
              {/each}
            </div>

            {@render group(t('Transparency'))}
            {#if app.materials.length}
              {@render toggle(t('Transparency effects'), t('Window backgrounds are see-through and tinted with the color above.'), draft.transparency, (v) => (draft!.transparency = v))}
              <div class="flex gap-4 flex-wrap pl-[42px] {draft.transparency ? '' : 'opacity-45 pointer-events-none'}">
                {#if app.materials.length > 1}
                  <label class="flex flex-col gap-1">
                    <span class="label">{t('Material')}</span>
                    <select class="field !w-[180px]" bind:value={draft.backdropMaterial}>
                      {#each app.materials as m}<option value={m}>{MATERIALS[m] ?? m}</option>{/each}
                    </select>
                  </label>
                {/if}
                <label class="flex flex-col gap-1">
                  <span class="label">{t('Strength')}</span>
                  <select class="field !w-[180px]" bind:value={draft.transparencyLevel}>
                    <option value={0}>{t('Light')}</option>
                    <option value={1}>{t('Medium')}</option>
                    <option value={2}>{t('Strong')}</option>
                  </select>
                </label>
              </div>
            {:else}
              <p class="text-[12px] text-muted">{t('This system does not support transparent windows, so Multron Download Manager uses solid backgrounds. Theme and color still apply.')}</p>
            {/if}
            <p class="text-[11px] text-dim mt-3">{t('Changes are previewed right away; press Save to keep them.')}</p>
          {:else if current.id === 'language'}
            {@render row('App language', 'By default Multron Download Manager uses your Windows, macOS or Linux language. Pick a language here to change it.')}
            <div class="grid grid-cols-[repeat(auto-fill,minmax(170px,1fr))] gap-1.5 mt-2">
              {#each [{ code: 'auto', name: t('System language') }, ...LANGUAGES] as l (l.code)}
                <button
                  type="button"
                  class="h-[36px] px-3 rounded-[8px] border text-[12.5px] flex items-center gap-2 cursor-pointer transition-colors text-left {draft.language === l.code ? 'border-accent/60 bg-raised text-white' : 'border-line text-muted hover:bg-hover hover:text-white'}"
                  onclick={() => (draft!.language = l.code)}
                >
                  <span class="w-[14px] h-[14px] rounded-full border flex-shrink-0 flex items-center justify-center {draft.language === l.code ? 'border-accent' : 'border-line'}">
                    {#if draft.language === l.code}<span class="w-[6px] h-[6px] rounded-full bg-accent"></span>{/if}
                  </span>
                  <span class="truncate">{l.name}</span>
                  {#if 'english' in l && l.english && l.english !== l.name}<span class="ml-auto pl-1 text-[11px] text-dim truncate">{l.english}</span>{/if}
                </button>
              {/each}
            </div>
            {@render group('Language files')}
            <p class="text-[12px] text-muted leading-relaxed">{t('Every language is a separate .ini file in the lang folder where Multron Download Manager is installed. To add a language, copy en.ini, name the copy after the language code (for example pl.ini) and translate the text after each “=”. Press Reload to use new or changed files.')}</p>
            <div class="flex gap-2 mt-2">
              <button type="button" class="btn" onclick={async () => { try { await openLanguageFolder(); } catch (e) { app.toast('error', errorText(e)); } }}><Icon name="folder" size={13} />{t('Open language folder')}</button>
              <button type="button" class="btn" onclick={async () => { await reloadLanguages(); app.toast('success', t('{count} languages loaded', { count: LANGUAGES.length })); }}><Icon name="refresh" size={13} />{t('Reload')}</button>
            </div>
            {@render group('Browser extension')}
            <p class="text-[12px] text-muted leading-relaxed">{t('The browser extension uses the same language as the app. After you save, it switches the next time it connects, within a few seconds. Open its menu again to see the change.')}</p>
            {@render group('Category folders')}
            <p class="text-[12px] text-muted leading-relaxed">{t('The Video, Music, Documents and other category folders are named in the app language. When you change the language, Multron Download Manager offers to rename the folders it created.')}</p>
          {:else if current.id === 'general'}
            {@render toggle('Keyboard shortcuts and command palette', 'On by default. Ctrl+K opens a search for every action and setting. Ctrl+N new download, Ctrl+F search, Ctrl+L LinkGrabber, Ctrl+H history, Ctrl+, settings, Ctrl+Shift+P pause all, Ctrl+Shift+R resume all, Space pause or resume the selection, Delete remove it.', draft.shortcutsEnabled, (v) => (draft!.shortcutsEnabled = v))}
            {@render toggle('Keep the computer awake while downloading', 'On by default. Windows does not go to sleep while a download is running. The screen can still turn off.', draft.preventSleep, (v) => (draft!.preventSleep = v))}
            {@render group('Startup')}
            {@render toggle(startupLabel, startupHint, draft.launchAtStartup, (v) => (draft!.launchAtStartup = v))}
            {@render toggle('Start minimized to the tray', '', draft.startMinimized, (v) => (draft!.startMinimized = v))}
            {@render toggle('Resume unfinished downloads on startup', '', draft.resumeOnStartup, (v) => (draft!.resumeOnStartup = v))}
            {@render group('Shutdown')}
            {@render toggle('Keep running in the background when the window is closed', 'Downloads, seeding and the browser bridge keep working. Quit from the tray icon.', draft.closeToTray, (v) => (draft!.closeToTray = v))}
            {@render toggle('Show notifications', 'When downloads finish, fail or a SHA-256 check completes.', draft.notifications, (v) => (draft!.notifications = v))}
            <div class="flex items-start gap-2">
              <div class="flex-1">{@render toggle('Play a sound when a download finishes', 'On by default. A short chime plays when a download is complete.', draft.soundOnComplete, (v) => (draft!.soundOnComplete = v))}</div>
              <button type="button" class="btn !h-[26px] !text-[11.5px] mt-[6px]" onclick={() => doneSound()}>{t('Test')}</button>
            </div>
            <div class="flex items-start gap-2">
              <div class="flex-1">{@render toggle('Play a sound when a download fails', 'A lower tone plays when a download fails.', draft.soundOnFail, (v) => (draft!.soundOnFail = v))}</div>
              <button type="button" class="btn !h-[26px] !text-[11.5px] mt-[6px]" onclick={() => doneSound(true)}>{t('Test')}</button>
            </div>
            {@render toggle('Show a notice when a download starts or ends', 'A card in the corner of the screen when a download starts, finishes or fails, also when Multron Download Manager is in the background or minimized.', draft.downloadNotices, (v) => (draft!.downloadNotices = v))}
            {@render toggle('Show a notice when a scan finishes', 'A card in the corner of the screen with the result of the Multron Cloud scan.', draft.scanNotices, (v) => (draft!.scanNotices = v))}
            {@render toggle('Also show notices inside the window', 'Off by default. While the window is in front, the cards appear inside it instead of the corner of the screen.', draft.inAppNotices, (v) => (draft!.inAppNotices = v))}
          {:else if current.id === 'files'}
            {@render row('Default download folder')}
            <div class="flex gap-2 mt-1">
              <input class="field" bind:value={draft.downloadDir} />
              <button class="btn" onclick={browseDir}><Icon name="folder" size={13} />{t('Browse')}</button>
            </div>
            {@render toggle('Link grabber: watch the clipboard for links', 'Off by default. Links you copy (http, https and magnet, also several at once) are collected, and a card offers to add them, like JDownloader’s link grabber. Links already in the list are skipped.', draft.clipboardMonitor, (v) => (draft!.clipboardMonitor = v))}
            {@render toggle('Delete downloaded files when removing a download', 'Ticks “Also delete the downloaded files” in the remove dialog by default, so removing a download also deletes its files from the disk. You can still untick it each time.', draft.deleteFilesByDefault, (v) => (draft!.deleteFilesByDefault = v))}
            {@render toggle('Sort into category folders', 'Video, Music, Documents, Archives, Programs, Images and Other sub-folders.', draft.organizeByCategory, (v) => (draft!.organizeByCategory = v))}
            {#if draft.organizeByCategory}
              {@render row('Category folders', 'Leave a folder empty to use a sub-folder named after the category in the download folder. The names follow the app language.')}
              <div class="space-y-1.5 mt-2">
                {#each CATEGORY_NAMES as [key, label]}
                  <div class="flex items-center gap-2">
                    <span class="w-[110px] text-[12px] text-muted flex-shrink-0">{t(label)}</span>
                    <input class="field flex-1 min-w-0" placeholder={`${draft.downloadDir}${sep}${t(label)}`} value={draft.categoryFolders[key] ?? ''} oninput={(e) => setCategoryFolder(key, e.currentTarget.value)} />
                    <button class="btn" title={t('Browse')} onclick={() => browseCategory(key)}><Icon name="folder" size={13} /></button>
                  </div>
                {/each}
              </div>
              {@render row('Custom categories', 'Files with these extensions go to their own folder, before the categories above.')}
              <div class="space-y-1.5 mt-2">
                {#each draft.customCategories as c, i}
                  <div class="flex items-center gap-2">
                    <input class="field !w-[120px]" placeholder={t('Name')} bind:value={c.name} />
                    <input class="field !w-[150px]" placeholder={t('Extensions, for example epub mobi')} value={c.extensions.join(' ')} oninput={(e) => (c.extensions = e.currentTarget.value.split(/[\s,]+/).filter(Boolean))} />
                    <input class="field flex-1 min-w-0" placeholder={t('Folder (empty: a sub-folder in the download folder)')} bind:value={c.folder} />
                    <button class="btn" title={t('Browse')} onclick={() => browseCustom(i)}><Icon name="folder" size={13} /></button>
                    <button class="btn" title={t('Remove')} onclick={() => (draft!.customCategories = draft!.customCategories.filter((_, j) => j !== i))}><Icon name="x" size={13} /></button>
                  </div>
                {/each}
                <button class="btn" onclick={() => (draft!.customCategories = [...draft!.customCategories, { name: '', extensions: [], folder: '' }])}><Icon name="plus" size={13} />{t('Add category')}</button>
              </div>
            {/if}
            <div class="flex items-center gap-3 mt-3">
              <button class="btn" onclick={resetCategories}><Icon name="refresh" size={13} />{t('Reset categories')}</button>
              <span class="text-[11px] text-muted leading-snug">{t('Removes your own categories and their icons, empties the category folders and puts the categories back in their original order and place in the sidebar.')}</span>
            </div>

            {@render group('Archives')}
            {@render toggle('Extract archives automatically', 'On by default. Finished ZIP, 7z and RAR downloads are unpacked by themselves. Split archives (name.part1.rar, name.7z.001) are unpacked when all of their parts have finished.', draft.autoExtract, (v) => (draft!.autoExtract = v))}
            <div class="pl-[42px] {draft.autoExtract ? '' : 'opacity-45 pointer-events-none'}">
              {@render toggle('Extract into a folder named after the archive', 'On by default, so the files of different archives do not mix.', draft.extractSubfolder, (v) => (draft!.extractSubfolder = v))}
              {@render toggle('Delete the archive after extracting', 'The archive and all of its parts are deleted once everything was extracted without errors.', draft.extractDeleteArchives, (v) => (draft!.extractDeleteArchives = v))}
              {#if extractTool === false}
                <p class="text-[11px] text-warn mt-1 leading-snug">{t('ZIP and 7z are extracted by Multron Download Manager itself. RAR and split archives need 7-Zip (Windows) or 7z / unrar (Linux and macOS), which was not found.')}</p>
              {/if}
            </div>
            {@render row('Passwords for archives', 'With one saved password, locked archives use it by themselves. With several, you choose which one to use, or they are tried one after another. Passwords that link-protection sites send with Click\'n\'Load are added here by themselves.')}
            <div class="mt-2 max-w-[560px] rounded-[10px] border border-line overflow-hidden">
              {#each draft.extractPasswords as _, i (i)}
                <div class="flex items-center gap-2 px-2.5 h-[38px] border-b border-line/60 bg-panel/40">
                  <span class="w-[18px] text-[11px] text-dim text-right flex-shrink-0">{i + 1}</span>
                  <Icon name="lock" size={12} class="text-muted flex-shrink-0" />
                  <input class="flex-1 min-w-0 h-[28px] px-2 bg-transparent text-[12.5px] mono outline-none rounded-[6px] focus:bg-bg" type={shownPasswords.has(i) ? 'text' : 'password'} autocomplete="off" bind:value={draft.extractPasswords[i]} />
                  <button class="icon-btn !w-7 !h-7" title={shownPasswords.has(i) ? t('Hide') : t('Show')} onclick={() => { const n = new Set(shownPasswords); if (n.has(i)) n.delete(i); else n.add(i); shownPasswords = n; }}><Icon name="eye" size={12} /></button>
                  <button class="icon-btn !w-7 !h-7" title={t('Move up')} disabled={i === 0} onclick={() => { const l = [...draft!.extractPasswords]; [l[i - 1], l[i]] = [l[i], l[i - 1]]; draft!.extractPasswords = l; }}><Icon name="chevronUp" size={12} /></button>
                  <button class="icon-btn !w-7 !h-7" title={t('Move down')} disabled={i === draft.extractPasswords.length - 1} onclick={() => { const l = [...draft!.extractPasswords]; [l[i + 1], l[i]] = [l[i], l[i + 1]]; draft!.extractPasswords = l; }}><Icon name="chevronDown" size={12} /></button>
                  <button class="icon-btn !w-7 !h-7 hover:!text-bad" title={t('Remove')} onclick={() => { draft!.extractPasswords = draft!.extractPasswords.filter((_, j) => j !== i); shownPasswords = new Set(); }}><Icon name="trash" size={12} /></button>
                </div>
              {:else}
                <p class="px-3 py-3 text-[12px] text-dim border-b border-line/60">{t('No passwords saved yet.')}</p>
              {/each}
              <div class="flex items-center gap-2 px-2.5 py-2">
                <input class="field !h-[30px] flex-1 min-w-0 mono text-[12px]" type="password" autocomplete="off" placeholder={t('New password…')} bind:value={newPassword} onkeydown={(e) => e.key === 'Enter' && addPassword()} />
                <button class="btn !h-[30px]" disabled={!newPassword.trim()} onclick={addPassword}><Icon name="plus" size={12} />{t('Add')}</button>
              </div>
            </div>
            {@render toggle('Ask which password to use when several are saved', 'On by default. A locked archive shows Choose a password in the list, where you pick one, type another or try them all. Turned off, the passwords are tried in the order above.', draft.extractAskPassword, (v) => (draft!.extractAskPassword = v))}
          {:else if current.id === 'connection'}
            {@render row('Connections per download', 'Each download is split into this many parallel connections (default 8). Idle connections take over the slowest ranges so the line stays saturated until the end. You can also change it per download while it runs.')}
            <div class="flex items-center gap-3 mt-2">
              <input type="range" min="1" max="64" bind:value={draft.threads} class="w-[300px] accent-[var(--color-accent-strong)]" />
              <input type="number" min="1" max="64" class="field !w-[80px]" bind:value={draft.threads} />
            </div>
            {@render toggle(t('Sequential download (one at a time)'), t('Direct and media downloads start one after another in queue order. Reorder them with the arrows on queued downloads. Same as the “One at a time” button at the top.'), draft.sequential, (v) => (draft!.sequential = v))}
            {@render row('Simultaneous downloads', 'How many direct/media downloads run at once; the rest wait in the queue. Torrents are not limited.')}
            <input type="number" min="1" max="32" class="field !w-[100px] mt-1" bind:value={draft.maxConcurrent} />
            {@render row('Retries per connection')}
            <input type="number" min="0" max="50" class="field !w-[100px] mt-1" bind:value={draft.retries} />
            {@render group('Failed downloads')}
            {@render row('Try failed downloads again automatically', 'Failed downloads are started again after the waiting time, as often as set. Downloads that failed for a reason a retry cannot fix (not found, login, CAPTCHA, SHA-256, threat) are left alone. 0 minutes = off.')}
            <div class="flex items-center gap-3 mt-1 flex-wrap">
              <label class="flex items-center gap-2 text-[12px]"><span class="text-muted">{t('Wait (minutes)')}</span><input type="number" min="0" max="1440" class="field !w-[90px]" bind:value={draft.autoRetryMinutes} /></label>
              <label class="flex items-center gap-2 text-[12px] {draft.autoRetryMinutes ? '' : 'opacity-45 pointer-events-none'}"><span class="text-muted">{t('At most (times)')}</span><input type="number" min="1" max="50" class="field !w-[80px]" bind:value={draft.autoRetryMax} /></label>
            </div>
            {@render group('Disk space')}
            {@render row('Pause downloads when free space drops below (GB)', 'Default 1 GB. Running downloads are paused before the drive fills up, and you get a warning. 0 = off.')}
            <input type="number" min="0" max="10000" class="field !w-[100px] mt-1" bind:value={draft.minFreeGb} />
            {@render row('User-Agent', 'Sent with direct downloads unless the browser extension provides its own.')}
            <input class="field mono mt-1" bind:value={draft.userAgent} />
          {:else if current.id === 'transfer'}
            {@render row('Global download speed limit (KB/s)', '0 = unlimited (use the full line). Applies to direct and media downloads; torrents have their own limits.')}
            <input type="number" min="0" class="field !w-[160px] mt-1" bind:value={draft.speedLimit} />
            <p class="text-[11px] text-muted mt-3">{t('Slower limits for certain hours are in Speed Scheduler.')}</p>
          {:else if current.id === 'sitelimits'}
            <p class="text-[11.5px] text-muted leading-relaxed mb-2">{t('Give a site its own speed limit or a maximum number of connections, for example to keep a slow host from being overloaded or to leave room for other downloads. A speed set on the download itself always wins. Subdomains are included.')}</p>
            {#each draft.siteLimits as l, i (i)}
              <div class="grid grid-cols-[22px_minmax(0,1.4fr)_130px_130px_auto] gap-2 items-center py-1.5 border-b border-line/40">
                <input type="checkbox" class="check" bind:checked={l.enabled} />
                <input class="field !h-[30px] mono text-[12px]" bind:value={l.host} />
                <label class="flex items-center gap-1.5 text-[11px] text-muted"><input type="number" min="0" class="field !h-[30px] !w-[80px] text-[12px]" bind:value={l.speedKb} />KB/s</label>
                <label class="flex items-center gap-1.5 text-[11px] text-muted"><input type="number" min="0" max="64" class="field !h-[30px] !w-[64px] text-[12px]" bind:value={l.maxConnections} />{t('conn.')}</label>
                <button class="icon-btn hover:!text-bad" title={t('Remove')} onclick={() => (draft!.siteLimits = draft!.siteLimits.filter((_, j) => j !== i))}><Icon name="trash" size={12} /></button>
              </div>
            {:else}
              <p class="py-3 text-[12px] text-muted">{t('No site limits yet.')}</p>
            {/each}
            <div class="grid grid-cols-[minmax(0,1.4fr)_130px_130px_auto] gap-2 items-center mt-3">
              <input class="field !h-[30px] mono text-[12px]" placeholder={t('Site, for example files.example.com')} bind:value={newLimit.host} onkeydown={(e) => e.key === 'Enter' && addLimit()} />
              <label class="flex items-center gap-1.5 text-[11px] text-muted"><input type="number" min="0" class="field !h-[30px] !w-[80px] text-[12px]" bind:value={newLimit.speedKb} />KB/s</label>
              <label class="flex items-center gap-1.5 text-[11px] text-muted"><input type="number" min="0" max="64" class="field !h-[30px] !w-[64px] text-[12px]" bind:value={newLimit.maxConnections} />{t('conn.')}</label>
              <button class="btn !h-[30px]" disabled={!newLimit.host.trim()} onclick={addLimit}><Icon name="plus" size={12} />{t('Add site')}</button>
            </div>
            <p class="text-[11px] text-muted mt-2">{t('0 = no limit. The limits apply to direct downloads.')}</p>
          {:else if current.id === 'rules'}
            <p class="text-[11.5px] text-muted leading-relaxed mb-2">{t('When a download finishes, the first matching rule renames it or moves it to another folder. Rules are checked from top to bottom. Torrents are left alone so they can keep seeding.')}</p>
            {#each draft.fileRules as r, i (i)}
              <div class="rounded-[10px] border border-line p-3 mb-2 space-y-2 {r.enabled ? '' : 'opacity-60'}">
                <div class="flex items-center gap-2 flex-wrap">
                  <input type="checkbox" class="check" bind:checked={r.enabled} />
                  <span class="text-[12px] text-muted">{t('When')}</span>
                  <select class="field !h-[30px] !w-auto !text-[12px]" bind:value={r.field}>
                    <option value="ext">{t('the file type is')}</option>
                    <option value="host">{t('the site is')}</option>
                    <option value="name">{t('the name contains')}</option>
                  </select>
                  <input class="field !h-[30px] flex-1 min-w-[140px] mono text-[12px]" bind:value={r.pattern} placeholder={r.field === 'ext' ? 'pdf epub' : r.field === 'host' ? 'files.example.com' : '*S01E??*'} />
                  <div class="flex-1"></div>
                  <button class="icon-btn" title={t('Move up')} disabled={i === 0} onclick={() => moveRule(i, -1)}><Icon name="chevronUp" size={12} /></button>
                  <button class="icon-btn" title={t('Move down')} disabled={i === draft.fileRules.length - 1} onclick={() => moveRule(i, 1)}><Icon name="chevronDown" size={12} /></button>
                  <button class="icon-btn hover:!text-bad" title={t('Remove')} onclick={() => (draft!.fileRules = draft!.fileRules.filter((_, j) => j !== i))}><Icon name="trash" size={12} /></button>
                </div>
                <div class="flex items-center gap-2 flex-wrap pl-[26px]">
                  <select class="field !h-[30px] !w-auto !text-[12px]" bind:value={r.action}>
                    <option value="move">{t('move it to')}</option>
                    <option value="rename">{t('rename it to')}</option>
                  </select>
                  <input class="field !h-[30px] flex-1 min-w-[180px] {r.action === 'rename' ? 'mono' : ''} text-[12px]" bind:value={r.target} placeholder={r.action === 'rename' ? '{date} {name}' : t('Choose a folder')} />
                  {#if r.action === 'move'}<button class="btn !h-[30px]" onclick={() => browseRule(i)}><Icon name="folder" size={12} /></button>{/if}
                </div>
              </div>
            {:else}
              <p class="py-3 text-[12px] text-muted">{t('No rules yet.')}</p>
            {/each}
            <button class="btn mt-1" onclick={addFileRule}><Icon name="plus" size={12} />{t('Add rule')}</button>
            <p class="text-[11px] text-muted mt-3 leading-relaxed">{t('In the name contains field, * stands for any text and ? for one character. In new names you can use {name}, {ext}, {date}, {host} and {category}; the extension is kept when {ext} is not used.')}</p>
          {:else if current.id === 'siterules'}
            <p class="text-[11.5px] text-muted leading-relaxed mb-3">{t('Teach Multron Download Manager how to find the download links on a website. A rule matches a site and picks links from its pages with a pattern (a regular expression; the first group in brackets is the link). LinkGrabber uses the rule when it scans a page of that site, and the rule can also turn a page link you add into its real download link. Rules only read pages; they cannot run code.')}</p>
            <div class="flex items-center gap-2 mb-3">
              <button class="btn btn-accent" onclick={() => (draft!.siteRules = [...draft!.siteRules, blankRule()])}><Icon name="plus" size={12} />{t('Add a rule')}</button>
              <button class="btn" disabled={!draft.siteRules.length} onclick={copyRules}><Icon name="code" size={12} />{t('Copy rules')}</button>
              <button class="btn" onclick={pasteRules}><Icon name="down" size={12} />{t('Paste rules')}</button>
            </div>
            <div class="space-y-3 max-w-[760px]">
              {#each draft.siteRules as r, i (i)}
                <div class="rounded-[12px] border border-line p-3.5 space-y-2.5 {r.enabled ? '' : 'opacity-60'}">
                  <div class="flex items-center gap-2">
                    <input type="checkbox" class="toggle" title={t('Use this rule')} bind:checked={r.enabled} />
                    <input class="field !h-[30px] flex-1 min-w-0 text-[12.5px] font-medium" placeholder={t('Rule name, for example My file host')} bind:value={r.name} />
                    <button class="icon-btn" title={t('Move up')} disabled={i === 0} onclick={() => { const l = [...draft!.siteRules]; [l[i - 1], l[i]] = [l[i], l[i - 1]]; draft!.siteRules = l; }}><Icon name="chevronUp" size={12} /></button>
                    <button class="icon-btn" title={t('Move down')} disabled={i === draft.siteRules.length - 1} onclick={() => { const l = [...draft!.siteRules]; [l[i + 1], l[i]] = [l[i], l[i + 1]]; draft!.siteRules = l; }}><Icon name="chevronDown" size={12} /></button>
                    <button class="icon-btn hover:!text-bad" title={t('Remove')} onclick={() => { draft!.siteRules = draft!.siteRules.filter((_, j) => j !== i); ruleTest = null; }}><Icon name="trash" size={12} /></button>
                  </div>
                  <div class="grid grid-cols-[110px_minmax(0,1fr)] gap-x-3 gap-y-2 items-center text-[12px]">
                    <span class="text-muted">{t('Site')}</span>
                    <input class="field !h-[30px] mono text-[12px]" placeholder="files.example.com" bind:value={r.host} />
                    <span class="text-muted">{t('Link pattern')}</span>
                    <input class="field !h-[30px] mono text-[12px]" placeholder={'href="(https://cdn\.example\.com/[^"]+)"'} bind:value={r.pattern} />
                  </div>
                  <div class="flex flex-wrap gap-x-5 gap-y-1.5 pl-[122px] text-[12px] text-muted">
                    <label class="flex items-center gap-1.5 cursor-pointer" title={t('Off: the links the rule finds are added to everything LinkGrabber finds on the page.')}><input type="checkbox" class="check" bind:checked={r.onlyRule} />{t('Only the links the rule finds')}</label>
                    <label class="flex items-center gap-1.5 cursor-pointer" title={t('Opens each link the rule finds (up to 20) and uses the pattern again there. For sites with a download page in between.')}><input type="checkbox" class="check" bind:checked={r.follow} />{t('Follow the found links one step')}</label>
                    <label class="flex items-center gap-1.5 cursor-pointer" title={t('When you add a page link of this site as a download, the first link the rule finds is downloaded instead.')}><input type="checkbox" class="check" bind:checked={r.onAdd} />{t('Use when adding a download')}</label>
                  </div>
                  <div class="pl-[122px]">
                    {#if ruleTest?.index === i}
                      <div class="flex gap-2">
                        <input class="field !h-[30px] flex-1 min-w-0 text-[12px]" placeholder={t('A page address of this site to test the rule on')} bind:value={ruleTest.url} onkeydown={(e) => e.key === 'Enter' && testRule(i)} />
                        <button class="btn !h-[30px]" disabled={!ruleTest.url.trim() || ruleTest.busy || !r.pattern.trim()} onclick={() => testRule(i)}><Icon name={ruleTest.busy ? 'refresh' : 'play'} size={12} class={ruleTest.busy ? 'animate-spin' : ''} />{t('Test')}</button>
                        <button class="icon-btn" aria-label={t('Close')} onclick={() => (ruleTest = null)}><Icon name="x" size={12} /></button>
                      </div>
                      {#if ruleTest.error}<p class="text-[11.5px] text-bad mt-1.5">{ruleTest.error}</p>{/if}
                      {#if ruleTest.result}
                        <div class="mt-1.5 rounded-[8px] border border-line max-h-[160px] overflow-y-auto">
                          <div class="px-2.5 py-1 text-[11px] text-dim border-b border-line/60">{t('{count} links found', { count: ruleTest.result.length })}</div>
                          {#each ruleTest.result.slice(0, 200) as u (u)}<div class="px-2.5 py-1 text-[11px] mono truncate" title={u}>{u}</div>{/each}
                        </div>
                      {/if}
                    {:else}
                      <button class="text-[11.5px] text-accent hover:underline" onclick={() => (ruleTest = { index: i, url: r.host ? `https://${r.host}/` : '', busy: false, result: null, error: null })}>{t('Test the rule on a page…')}</button>
                    {/if}
                  </div>
                </div>
              {:else}
                <p class="text-[12px] text-muted">{t('No site rules yet. Most sites work without one; add a rule for a site whose files LinkGrabber does not find.')}</p>
              {/each}
            </div>
          {:else if current.id === 'backup'}
            <p class="text-[11.5px] text-muted leading-relaxed mb-3">{t('Save your settings, categories, rules, site logins, the download list and the download history into one file, and restore them later or on another computer.')}</p>
            <div class="rounded-[12px] border border-line p-4 space-y-3 max-w-[620px]">
              <div class="flex items-center gap-3">
                <div class="w-[36px] h-[36px] rounded-[10px] bg-accent/15 text-accent flex items-center justify-center"><Icon name="down" size={16} /></div>
                <div class="flex-1"><div class="text-[13px] font-semibold">{t('Create a backup')}</div><div class="text-[11.5px] text-muted">{t('Saves everything into a .json file.')}</div></div>
                <button class="btn btn-primary" disabled={backupBusy} onclick={exportBackup}>{t('Save backup…')}</button>
              </div>
              <label class="flex items-start gap-2 text-[12px] cursor-pointer pl-[48px]"><input type="checkbox" class="check mt-[2px]" bind:checked={backupSecrets} /><span>{t('Include passwords and keys')}<span class="block text-[11px] text-warn">{t('They are written into the file as plain text, so keep the file safe. Without this, saved passwords stay only on this computer.')}</span></span></label>
            </div>
            <div class="rounded-[12px] border border-line p-4 mt-3 max-w-[620px]">
              <div class="flex items-center gap-3">
                <div class="w-[36px] h-[36px] rounded-[10px] bg-accent/15 text-accent flex items-center justify-center"><Icon name="up" size={16} /></div>
                <div class="flex-1"><div class="text-[13px] font-semibold">{t('Restore a backup')}</div><div class="text-[11.5px] text-muted">{t('Replaces the settings and adds the downloads that are missing from the list.')}</div></div>
                <button class="btn" disabled={backupBusy} onclick={importBackup}>{t('Open backup…')}</button>
              </div>
            </div>
            <div class="rounded-[12px] border border-bad/30 p-4 mt-3 max-w-[620px]">
              <div class="flex items-center gap-3">
                <div class="w-[36px] h-[36px] rounded-[10px] bg-bad/15 text-bad flex items-center justify-center"><Icon name="refresh" size={16} /></div>
                <div class="flex-1"><div class="text-[13px] font-semibold">{t('Reset all settings')}</div><div class="text-[11.5px] text-muted">{t('Puts every setting back to its default. Downloads, history and the browser extension connection stay.')}</div></div>
                <button class="btn hover:!text-bad" onclick={resetAllSettings}>{t('Reset…')}</button>
              </div>
            </div>
            <div class="rounded-[12px] border border-line p-4 mt-3 max-w-[620px] space-y-3">
              <div class="flex items-center gap-3">
                <div class="w-[36px] h-[36px] rounded-[10px] bg-accent/15 text-accent flex items-center justify-center"><Icon name="clock" size={16} /></div>
                <div class="flex-1"><div class="text-[13px] font-semibold">{t('Automatic backup')}</div><div class="text-[11.5px] text-muted">{t('Off by default. Saves a backup by itself on a schedule, without passwords and keys.')}</div></div>
                <input type="checkbox" class="toggle" aria-label={t('Automatic backup')} bind:checked={draft.autoBackup} />
              </div>
              <div class="pl-[48px] space-y-3 {draft.autoBackup ? '' : 'opacity-45 pointer-events-none'}">
                <div class="flex flex-wrap gap-1.5">
                  {#each BACKUP_PRESETS as p (p.id)}
                    <button type="button" class="h-[30px] px-3 rounded-[8px] border text-[12px] cursor-pointer {backupPreset === p.id && !customBackup ? 'border-accent/60 bg-raised text-white' : 'border-line text-muted hover:bg-hover hover:text-white'}" onclick={() => { draft!.autoBackupCount = p.count; draft!.autoBackupUnit = p.unit; customBackup = false; }}>{t(p.label)}</button>
                  {/each}
                  <button type="button" class="h-[30px] px-3 rounded-[8px] border text-[12px] cursor-pointer {backupPreset === 'custom' || customBackup ? 'border-accent/60 bg-raised text-white' : 'border-line text-muted hover:bg-hover hover:text-white'}" onclick={() => (customBackup = true)}>{t('Custom')}</button>
                </div>
                {#if backupPreset === 'custom' || customBackup}
                  <div class="flex items-center gap-2 text-[12px] text-muted">
                    <span>{t('Every')}</span>
                    <input type="number" min="1" max="120" class="field !h-[30px] !w-[72px] text-[12px]" bind:value={draft.autoBackupCount} />
                    <select class="field !h-[30px] !w-auto text-[12px]" bind:value={draft.autoBackupUnit}>
                      <option value="day">{t('days')}</option>
                      <option value="week">{t('weeks')}</option>
                      <option value="month">{t('months')}</option>
                      <option value="year">{t('years')}</option>
                    </select>
                  </div>
                {/if}
                <div class="flex items-center gap-2 text-[12px] text-muted">
                  <span>{t('Keep the last')}</span>
                  <input type="number" min="1" max="100" class="field !h-[30px] !w-[72px] text-[12px]" bind:value={draft.autoBackupKeep} />
                  <span>{t('backups; older ones are deleted.')}</span>
                </div>
                <div>
                  <span class="label">{t('Backup folder')}</span>
                  <div class="flex gap-2 mt-1">
                    <input class="field !h-[30px] flex-1 min-w-0 text-[12px]" placeholder={backupDir} bind:value={draft.autoBackupDir} />
                    <button class="btn !h-[30px]" onclick={browseBackupDir}><Icon name="folder" size={12} /></button>
                  </div>
                </div>
              </div>
              <div class="pl-[48px] flex items-center gap-2 flex-wrap">
                <button class="btn !h-[30px]" disabled={backingUp} onclick={backupNow}><Icon name={backingUp ? 'refresh' : 'down'} size={12} class={backingUp ? 'animate-spin' : ''} />{t('Back up now')}</button>
                {#if backupDir}<button class="btn !h-[30px]" onclick={() => api.openPath(backupDir).catch(() => app.toast('info', t('No backups yet.')))}><Icon name="folderOpen" size={12} />{t('Open folder')}</button>{/if}
                <span class="text-[11.5px] text-dim">{backupFiles.length ? t('Last backup: {date}', { date: new Date(backupFiles[0].created).toLocaleString() }) : t('No backups yet.')}</span>
              </div>
              {#if backupFiles.length}
                <div class="pl-[48px]">
                  <div class="rounded-[10px] border border-line max-h-[180px] overflow-y-auto">
                    {#each backupFiles as b (b.path)}
                      <div class="flex items-center gap-2 px-3 h-[32px] border-b border-line/50 text-[12px]">
                        <Icon name="archive" size={12} class="text-muted" />
                        <span class="flex-1 truncate" title={b.path}>{new Date(b.created).toLocaleString()}</span>
                        <span class="text-[11px] text-dim">{bytes(b.size)}</span>
                      </div>
                    {/each}
                  </div>
                  <p class="text-[11px] text-dim mt-1">{t('Restore one with Open backup… above.')}</p>
                </div>
              {/if}
            </div>
          {:else if current.id === 'logins'}
            <p class="text-[11.5px] text-muted leading-relaxed mb-2">{t('Save a login once and every download from that site, including several links at once and LinkGrabber, logs in by itself. Logins are only sent over HTTPS, and passwords are encrypted with your Windows account.')}</p>
            {@render toggle('Use saved logins automatically', 'On by default. Downloads from a saved site use its login without asking. A login typed in the new download window always wins.', draft.loginsAuto, (v) => (draft!.loginsAuto = v))}
            {@render group('Saved sites')}
            {#each draft.siteLogins as l, i (i)}
              <div class="grid grid-cols-[22px_minmax(0,1.2fr)_minmax(0,1fr)_minmax(0,1fr)_90px_auto] gap-2 items-center py-1.5 border-b border-line/40">
                <input type="checkbox" class="check" title={t('Use this login')} bind:checked={l.enabled} />
                <input class="field !h-[30px] mono text-[12px]" bind:value={l.host} placeholder="files.example.com" />
                <input class="field !h-[30px] text-[12px]" bind:value={l.user} placeholder={l.kind === 'bearer' ? t('(not needed)') : t('User name')} disabled={l.kind === 'bearer'} />
                <input class="field !h-[30px] text-[12px]" type={showPasswords ? 'text' : 'password'} bind:value={l.password} placeholder={l.kind === 'bearer' ? t('Token') : t('Password')} />
                <select class="field !h-[30px] !text-[11.5px]" bind:value={l.kind}><option value="basic">{t('Password')}</option><option value="bearer">{t('Token')}</option></select>
                <div class="flex items-center gap-1">
                  <label class="flex items-center gap-1 text-[11px] text-muted whitespace-nowrap" title={t('Also use it for subdomains such as cdn.{site}', { site: l.host || 'example.com' })}><input type="checkbox" class="check" bind:checked={l.subdomains} />{t('Subdomains')}</label>
                  <button class="icon-btn hover:!text-bad" title={t('Remove')} onclick={() => (draft!.siteLogins = draft!.siteLogins.filter((_, j) => j !== i))}><Icon name="trash" size={12} /></button>
                </div>
              </div>
            {:else}
              <p class="py-3 text-[12px] text-muted">{t('No saved sites yet. Add one below, or tick “Remember for this site” when a download asks for a login.')}</p>
            {/each}
            <div class="grid grid-cols-[minmax(0,1.2fr)_minmax(0,1fr)_minmax(0,1fr)_90px_auto] gap-2 items-center mt-3">
              <input class="field !h-[30px] mono text-[12px]" bind:value={newLogin.host} placeholder={t('Site, for example files.example.com')} onkeydown={(e) => e.key === 'Enter' && addLogin()} />
              <input class="field !h-[30px] text-[12px]" bind:value={newLogin.user} placeholder={t('User name')} disabled={newLogin.kind === 'bearer'} />
              <input class="field !h-[30px] text-[12px]" type={showPasswords ? 'text' : 'password'} bind:value={newLogin.password} placeholder={newLogin.kind === 'bearer' ? t('Token') : t('Password')} onkeydown={(e) => e.key === 'Enter' && addLogin()} />
              <select class="field !h-[30px] !text-[11.5px]" bind:value={newLogin.kind}><option value="basic">{t('Password')}</option><option value="bearer">{t('Token')}</option></select>
              <button class="btn !h-[30px]" disabled={!newLogin.host.trim()} onclick={addLogin}><Icon name="plus" size={12} />{t('Add site')}</button>
            </div>
            <label class="flex items-center gap-2 text-[11.5px] text-muted mt-2 cursor-pointer"><input type="checkbox" class="check" bind:checked={showPasswords} />{t('Show passwords')}</label>
          {:else if current.id === 'scheduler'}
            {@render group('Download scheduler')}
            {@render toggle('Start and stop downloads on a schedule', 'Off by default. At the start time the queue starts by itself, for example at night when the line is free. With “Download later” in the new download window you put downloads into this queue without starting them.', draft.schedulerEnabled, (v) => (draft!.schedulerEnabled = v))}
            <div class="space-y-3 mt-2 {draft.schedulerEnabled ? '' : 'opacity-45 pointer-events-none'}">
              <div class="flex items-center gap-3 flex-wrap">
                <label class="flex items-center gap-2"><span class="label">{t('Start at')}</span><input type="time" class="field !w-[120px]" bind:value={draft.schedulerStart} /></label>
                <label class="flex items-center gap-2 text-[12px]"><input type="checkbox" class="check" bind:checked={draft.schedulerStopEnabled} />{t('Stop at')}</label>
                <input type="time" class="field !w-[120px] {draft.schedulerStopEnabled ? '' : 'opacity-45 pointer-events-none'}" bind:value={draft.schedulerStop} />
              </div>
              <div class="flex gap-1.5 flex-wrap">
                {#each DAYS as day, i}
                  <button type="button" class="h-[28px] px-3 rounded-full border text-[12px] cursor-pointer {draft.schedulerDays.includes(i) ? 'border-accent/60 bg-raised text-white' : 'border-line text-muted hover:bg-hover'}" onclick={() => toggleSchedulerDay(i)}>{t(day)}</button>
                {/each}
              </div>
              <div class="flex gap-2 flex-wrap">
                {#each [{ id: 'later', label: 'Only downloads added with “Download later”' }, { id: 'all', label: 'Every paused download' }] as opt}
                  <button type="button" class="h-[32px] px-3 rounded-[8px] border text-[12px] cursor-pointer transition-colors {draft.schedulerScope === opt.id ? 'border-accent/60 bg-raised text-white' : 'border-line text-muted hover:bg-hover hover:text-white'}" onclick={() => (draft!.schedulerScope = opt.id as 'later' | 'all')}>{t(opt.label)}</button>
                {/each}
              </div>
              <p class="text-[11px] text-muted">{t('The queue keeps its order and runs as many downloads at once as Connection → simultaneous downloads allows. “When downloads finish” can shut down or put the computer to sleep afterwards.')}</p>
            </div>
            {@render group('Speed limits')}
            {@render toggle('Use alternative speed limits now', 'Same as the turtle button in the status bar.', draft.altSpeedEnabled, (v) => (draft!.altSpeedEnabled = v))}
            <div class="grid grid-cols-2 gap-4 max-w-[420px] mt-1">
              <label class="flex flex-col gap-1"><span class="label">{t('Alternative download limit (KB/s)')}</span><input type="number" min="0" class="field" bind:value={draft.altDownloadLimit} /></label>
              <label class="flex flex-col gap-1"><span class="label">{t('Alternative upload limit (KB/s)')}</span><input type="number" min="0" class="field" bind:value={draft.altUploadLimit} /></label>
            </div>
            <p class="text-[11px] text-muted mt-1">{t('Apply to every download and torrent while active. 0 = unlimited.')}</p>
            {@render group('Schedule')}
            {@render toggle('Switch to alternative limits automatically', 'For example during working hours or at night.', draft.altScheduleEnabled, (v) => (draft!.altScheduleEnabled = v))}
            <div class="flex items-center gap-3 mt-2 {draft.altScheduleEnabled ? '' : 'opacity-45 pointer-events-none'}">
              <label class="flex items-center gap-2"><span class="label">{t('From')}</span><input type="time" class="field !w-[120px]" bind:value={draft.altScheduleFrom} /></label>
              <label class="flex items-center gap-2"><span class="label">{t('To')}</span><input type="time" class="field !w-[120px]" bind:value={draft.altScheduleTo} /></label>
            </div>
            <div class="flex gap-1.5 mt-3 flex-wrap {draft.altScheduleEnabled ? '' : 'opacity-45 pointer-events-none'}">
              {#each DAYS as day, i}
                <button type="button" class="h-[28px] px-3 rounded-full border text-[12px] cursor-pointer {draft.altScheduleDays.includes(i) ? 'border-accent/60 bg-raised text-white' : 'border-line text-muted hover:bg-hover'}" onclick={() => toggleDay(i)}>{t(day)}</button>
              {/each}
            </div>
            <p class="text-[11px] text-muted mt-2">{t('A range that ends earlier than it starts runs over midnight.')}</p>
          {:else if current.id === 'integrity'}
            {@render toggle('Ask for a SHA-256 before every download', 'The New Task dialog asks for the expected SHA-256 of each file (browser downloads open the dialog too). When the download finishes the file is hashed and compared automatically; a mismatch marks the download as failed.', draft.askSha256, (v) => (draft!.askSha256 = v))}
            {@render toggle('Find checksums automatically', 'Looks for "<file>.sha256", "SHA256SUMS" and similar files next to the download and fills the expected hash in for you.', draft.autoDetectSha256, (v) => (draft!.autoDetectSha256 = v))}
            <p class="text-[11px] text-muted mt-3">{t('Torrents are always verified piece by piece with the hashes in the torrent itself.')}</p>
          {:else if current.id === 'browser'}
            <div class="flex items-center gap-2 text-[12px] mb-2">
              <span class="w-[8px] h-[8px] rounded-full {bridge?.running ? 'bg-ok' : 'bg-bad'}"></span>
              {bridge?.running ? t('Listening on {address}', { address: `127.0.0.1:${bridge.port}` }) : bridge?.error ?? t('Not running')}
            </div>
            {@render toggle('Enable the browser extension bridge', 'A local-only endpoint (127.0.0.1) the Multron Download Manager extension uses to send downloads.', draft.bridgeEnabled, (v) => (draft!.bridgeEnabled = v))}
            {@render row('Bridge port', 'Must match the port set in the extension (default 6801).')}
            <input type="number" min="1024" max="65535" class="field !w-[120px] mt-1" bind:value={draft.bridgePort} />
            {@render toggle('Click\'n\'Load for link-protection sites', 'On by default. Link-protection sites (filecrypt, keeplinks, linkcrypt and others) have a Click\'n\'Load button that sends their links to a download manager. Their links then open here for review, together with the archive passwords. If such a page shows a CAPTCHA, solve it in the browser first; Multron Download Manager never solves CAPTCHAs by itself.', draft.clickNLoad, (v) => (draft!.clickNLoad = v))}
            <div class="pl-[42px] {draft.clickNLoad ? '' : 'opacity-45 pointer-events-none'}">
              {@render toggle('Only from known link-protection sites', 'Click\'n\'Load links are accepted only from known link-protection sites, not from any web page.', draft.clickNLoadKnownOnly, (v) => (draft!.clickNLoadKnownOnly = v))}
            </div>
            {@render toggle('Show a progress window for each browser download', 'On by default. Downloads the browser extension sends start right away and a small window shows their progress, speed and details, with a speed limiter and what to do when they finish. With Ask before every download on, that window asks first.', draft.progressWindow, (v) => (draft!.progressWindow = v))}
            {@render toggle('Ask before every download', 'On by default. Every download the browser extension sends opens its own small window, separate from the main window, where you can change the folder, the file name, the quality or the SHA-256. The download then starts in that window with its progress and details, and also appears in the main list.', draft.confirmBrowserDownloads, (v) => (draft!.confirmBrowserDownloads = v))}
            {@render toggle('Show only the options the site needs', 'On by default. On sites that only have sound, such as SoundCloud or Bandcamp, the extension menu and the new download window show only the audio formats, without video qualities or video formats.', draft.siteOptions, (v) => (draft!.siteOptions = v))}
            {@render toggle('Show the download icon in fullscreen videos', 'On by default. When a video plays in fullscreen, the browser extension shows a small download icon in the top-left corner. Its × button hides it on that page only; turn this off to hide it everywhere.', draft.fullscreenVideoButton, (v) => (draft!.fullscreenVideoButton = v))}
            {@render group('Install the extension')}
            <p class="text-[12px] text-muted leading-relaxed">
              {t('The setup assistant finds your browsers, prepares the extension folder, opens the extensions page and connects the extension automatically.')}
              {t('Connected extensions')}: <span class="text-white">{app.settings?.trustedExtensions.length ?? 0}</span>
            </p>
            {@render group('Installed browsers')}
            {#if installedBrowsers === null}
              <p class="text-[12px] text-muted">{t('Looking for browsers…')}</p>
            {:else if installedBrowsers.length === 0}
              <p class="text-[12px] text-muted">{t('No supported browser was found. Open the extension folder and load it from your browser’s extensions page.')}</p>
            {:else}
              <div class="grid grid-cols-2 gap-2 mt-1">
                {#each installedBrowsers as b (b.id)}
                  <div class="flex items-center gap-2 p-2 rounded-[9px] border border-ok/30 bg-ok/5 text-[12px]">
                    <Icon name="globe" size={15} class="text-ok flex-shrink-0" />
                    <div class="min-w-0 flex-1 leading-tight">
                      <div class="font-medium truncate" title={b.path}>{b.name}</div>
                      <div class="text-[10.5px] text-dim">{b.engine === 'firefox' ? 'Firefox' : 'Chromium'} · {t('Installed')}</div>
                    </div>
                    <button class="btn !h-[26px]" onclick={() => setupFor(b.id)}><Icon name="puzzle" size={12} />{t('Install extension')}</button>
                  </div>
                {/each}
              </div>
            {/if}
            {@render group('Connected extensions')}
            {#if !app.settings?.trustedExtensions.length}
              <p class="text-[12px] text-muted">{t('None yet.')}</p>
            {/if}
            {#if app.settings?.trustedExtensions.length}
              <div class="mt-3 space-y-1.5">
                {#each app.settings.trustedExtensions as o (o)}
                  <div class="flex items-center gap-2 p-2 rounded-[9px] border border-line bg-subtle/40 text-[12px]">
                    <Icon name="puzzle" size={13} class="text-accent flex-shrink-0" />
                    <div class="min-w-0 flex-1">
                      <div class="font-medium">{extensionBrowser(o)}</div>
                      <div class="mono text-dim truncate" title={o}>{o}</div>
                    </div>
                    <button class="btn !h-[26px]" onclick={() => (app.extensionSetupOpen = true)}><Icon name="refresh" size={12} />{t('Reinstall')}</button>
                    <button class="btn btn-danger !h-[26px]" onclick={() => removeExtension(o)}><Icon name="trash" size={12} />{t('Remove from browser')}</button>
                  </div>
                {/each}
              </div>
            {/if}
            <div class="flex gap-2 mt-3 flex-wrap">
              <button class="btn btn-accent" onclick={() => (app.extensionSetupOpen = true)}>
                <Icon name="puzzle" size={13} />{t('Open setup assistant')}
              </button>
              <button class="btn" onclick={() => app.run(async () => api.openPath(await api.extensionDir()))}>
                <Icon name="folderOpen" size={13} />{t('Open extension folder')}
              </button>
              <button class="btn btn-danger" onclick={removeEverywhere}>
                <Icon name="trash" size={13} />{t('Remove completely')}
              </button>
              <button class="btn" onclick={() => api.openUrl('https://github.com/Multron-Community/Multron-Download-Manager/releases/latest')}>
                <Icon name="globe" size={13} />{t('Open releases page')}
              </button>
            </div>
          {:else if current.id === 'media'}
            <div class="rounded-[10px] border border-line bg-panel p-3 text-[12px] space-y-2">
              <div class="flex items-center justify-between gap-3">
                <span>yt-dlp {tools?.ytdlpVersion ? `· ${tools.ytdlpVersion}` : tools?.ytdlpInstalled ? '' : '· ' + t('not installed (downloaded on first use)')}</span>
                <button class="btn" disabled={toolBusy !== null} onclick={() => runTool('ytdlp')}>
                  <Icon name="refresh" size={13} />{toolBusy === 'ytdlp' ? t('Downloading…') : tools?.ytdlpInstalled ? t('Update from GitHub') : t('Install from GitHub')}
                </button>
              </div>
              <div class="flex items-center justify-between gap-3">
                <span>FFmpeg · {tools?.ffmpegAvailable ? (tools.ffmpegManaged ? t('installed by Multron Download Manager') : t('found on this system')) : t('not found — needed for 1080p+ and MP3')}</span>
                {#if tools?.ffmpegAutoInstall}
                  <button class="btn" disabled={toolBusy !== null} onclick={() => runTool('ffmpeg')}>
                    <Icon name="down" size={13} />{toolBusy === 'ffmpeg' ? t('Downloading…') : tools?.ffmpegManaged ? t('Reinstall') : t('Install')}
                  </button>
                {:else if !tools?.ffmpegAvailable}
                  <span class="mono text-[11px] text-muted">sudo apt install ffmpeg · sudo dnf install ffmpeg · sudo pacman -S ffmpeg</span>
                {/if}
              </div>
              <div class="flex items-center justify-between gap-3">
                <span>Deno {tools?.denoVersion ? `· ${tools.denoVersion} · ` : '· '}{tools?.denoAvailable ? (tools.denoManaged ? t('installed by Multron Download Manager') : t('found on this system')) : t('not installed — installed with the first YouTube video, needed for full YouTube speed')}</span>
                {#if tools?.denoAutoInstall}
                  <button class="btn" disabled={toolBusy !== null} onclick={() => runTool('deno')}>
                    <Icon name="down" size={13} />{toolBusy === 'deno' ? t('Downloading…') : tools?.denoManaged ? t('Reinstall') : t('Install')}
                  </button>
                {/if}
              </div>
              {#if app.toolProgress}
                <div class="w-full h-[4px] bg-line rounded overflow-hidden">
                  <div class="h-full bg-accent" style="width: {app.toolProgress.total ? (app.toolProgress.downloaded / app.toolProgress.total) * 100 : 30}%"></div>
                </div>
              {/if}
            </div>
            <div class="flex items-start gap-2 p-3 rounded-[10px] border border-line bg-subtle/40 text-[12px] my-2">
              <Icon name="refresh" size={14} class="text-accent mt-[1px] flex-shrink-0" />
              <span class="text-muted leading-relaxed">{t('yt-dlp and FFmpeg are kept up to date automatically. Multron Download Manager checks for new versions when it starts and every few hours, and updates them while no video is downloading.')}</span>
            </div>
            {@render row('Default quality')}
            <select class="field !w-[220px] mt-1" bind:value={draft.mediaQuality}>
              {#each QUALITIES as q}
                <option value={q.id}>{t(q.label)}</option>
              {/each}
            </select>
            {@render toggle('Show only the qualities the video has', 'On by default. The quality list in the new download window and in the browser extension goes up to the highest quality of the video, and Best shows which one that is. A default quality the video does not have becomes Best.', draft.mediaAvailableQualities, (v) => (draft!.mediaAvailableQualities = v))}
            {@render row('Video container')}
            <select class="field !w-[220px] mt-1" bind:value={draft.mediaContainer}>
              {#each CONTAINERS as c}<option value={c.id}>{c.label}</option>{/each}
            </select>
            {@render row('Use cookies from browser', 'For private, age-restricted or login-only media (Instagram, private Reddit…). The extension already sends the page cookies; this is a fallback for links added by hand.')}
            <select class="field !w-[220px] mt-1" bind:value={draft.cookiesFromBrowser}>
              {#each browsers as b}<option value={b}>{b || 'None'}</option>{/each}
            </select>
            {@render toggle('Embed metadata', '', draft.embedMetadata, (v) => (draft!.embedMetadata = v))}
            {@render toggle('Embed thumbnail', '', draft.embedThumbnail, (v) => (draft!.embedThumbnail = v))}
          {:else if current.id === 'torrent'}
            {@render group('Adding torrents')}
            {@render row('Save torrents to', 'Empty = the normal download folder. When the new download window still shows the normal folder, torrents go here instead.')}
            <div class="flex gap-2 mt-1 max-w-[560px]">
              <input class="field flex-1 min-w-0" bind:value={draft.torrentDir} placeholder={draft.downloadDir} />
              <button type="button" class="btn" onclick={async () => { const r = await openDialog({ directory: true, defaultPath: draft?.torrentDir || draft?.downloadDir || undefined }); if (typeof r === 'string' && draft) draft.torrentDir = r; }}><Icon name="folder" size={13} />{t('Browse')}</button>
              {#if draft.torrentDir}<button type="button" class="btn" onclick={() => (draft!.torrentDir = '')}><Icon name="x" size={12} /></button>{/if}
            </div>
            {@render toggle('Add torrents paused', 'New torrents wait in the list until you start them, or until the scheduler starts them.', draft.torrentStartPaused, (v) => (draft!.torrentStartPaused = v))}
            {@render toggle('Leave extra files unticked', 'Samples, .nfo, .txt and similar files that come with many torrents are not selected in the new download window.', draft.torrentSkipEnabled, (v) => (draft!.torrentSkipEnabled = v))}
            <div class="pl-[42px] {draft.torrentSkipEnabled ? '' : 'opacity-45 pointer-events-none'}">
              <input class="field mono text-[12px] max-w-[520px]" bind:value={draft.torrentSkipPatterns} />
              <p class="text-[11px] text-muted mt-1">{t('Separate with commas. A word matches anywhere in the file name, an entry starting with a dot matches the end (.nfo).')}</p>
            </div>
            {@render group('Connection')}
            {@render row('Incoming TCP / UDP listen port', 'Default 22155. Pick a port between 49160 and 65534 or keep the default; avoid 6881–6889 which many ISPs throttle. If the port is busy a random one is used.')}
            <input type="number" min="1" max="65535" class="field !w-[140px] mt-1" bind:value={draft.torrentPort} />
            {@render toggle('UPnP port forwarding', 'Opens the port on your router automatically.', draft.torrentUpnp, (v) => (draft!.torrentUpnp = v))}
            {@render toggle('uTP (µTP) transport', 'UDP-based peer connections in addition to TCP.', draft.torrentUtp, (v) => (draft!.torrentUtp = v))}
            {@render toggle('DHT (trackerless peers)', '', draft.torrentDht, (v) => (draft!.torrentDht = v))}
            {@render toggle('Local Service Discovery', 'Finds peers on your local network.', draft.torrentLsd, (v) => (draft!.torrentLsd = v))}
            {@render row('Maximum peers per torrent', 'Default 500. More peers usually means faster downloads; lower it if your router struggles with many connections.')}
            <input type="number" min="10" max="2000" class="field !w-[120px] mt-1" bind:value={draft.torrentPeerLimit} />
            <p class="text-[11px] text-muted mt-3">{t('Changing these restarts the torrent engine; running torrents continue where they left off.')}</p>
          {:else if current.id === 'queue'}
            {@render row('Maximum active downloading torrents', 'The rest wait in the queue and start in order. 0 = no limit.')}
            <div class="flex items-center gap-2 mt-1">
              <input type="number" min="0" max="200" class="field !w-[120px]" bind:value={draft.torrentMaxActiveDownloads} />
              <button type="button" class="btn !h-[30px] {Number(draft.torrentMaxActiveDownloads) === 1 ? 'text-accent border-accent/50' : ''}" onclick={() => (draft!.torrentMaxActiveDownloads = 1)}>{t('One at a time')}</button>
            </div>
            <p class="text-[11px] text-muted mt-1">{t('With one at a time, torrents download one after another in the order of the list.')}</p>
            {@render row('Maximum active seeding torrents', '0 = no limit.')}
            <input type="number" min="0" max="500" class="field !w-[120px] mt-1" bind:value={draft.torrentMaxActiveSeeds} />
            <p class="text-[11px] text-muted mt-3">{t('Reorder queued torrents with the arrows on each download. Paused torrents do not count.')}</p>
          {:else if current.id === 'watch'}
            {@render toggle('Delete .torrent files from the folder after adding', 'Instead of renaming them to .torrent.added. A copy is kept by Multron Download Manager.', draft.torrentDeleteWatched, (v) => (draft!.torrentDeleteWatched = v))}
            {@render toggle('Add .torrent files from a folder automatically', 'Every .torrent file saved into the folder is added and renamed to .torrent.added.', draft.watchFolderEnabled, (v) => (draft!.watchFolderEnabled = v))}
            <div class="flex gap-2 mt-1 {draft.watchFolderEnabled ? '' : 'opacity-45 pointer-events-none'}">
              <input class="field" bind:value={draft.watchFolder} />
              <button class="btn" onclick={() => browseInto((p) => (draft!.watchFolder = p), draft!.watchFolder)}><Icon name="folderOpen" size={13} />{t('Browse')}</button>
            </div>
          {:else if current.id === 'rss'}
            {@render toggle('Check RSS feeds automatically', 'New items that match a rule are downloaded on their own.', draft.rssEnabled, (v) => (draft!.rssEnabled = v))}
            {@render row('Check every (minutes)')}
            <input type="number" min="5" max="1440" class="field !w-[120px] mt-1" bind:value={draft.rssInterval} />
            {@render group('Feeds')}
            {#each draft.rssFeeds as feed, i (feed.url)}
              <div class="flex items-center gap-2 py-1">
                <input type="checkbox" class="check" bind:checked={feed.enabled} />
                <input class="field !h-[28px] !w-[200px]" placeholder={t('Name')} bind:value={feed.name} />
                <span class="mono text-[11px] text-muted truncate flex-1" title={feed.url}>{feed.url}</span>
                <button class="icon-btn hover:!text-bad" title={t('Remove')} onclick={() => (draft!.rssFeeds = draft!.rssFeeds.filter((_, j) => j !== i))}><Icon name="trash" size={13} /></button>
              </div>
            {:else}
              <p class="text-[12px] text-muted py-1">{t('No feeds yet.')}</p>
            {/each}
            <div class="flex gap-2 mt-2">
              <input class="field" placeholder="https://…/rss" bind:value={newFeed} onkeydown={(e) => e.key === 'Enter' && addFeed()} />
              <button class="btn" onclick={addFeed}><Icon name="plus" size={12} />{t('Add feed')}</button>
            </div>
            {@render group('Download rules')}
            <p class="text-[11px] text-muted mb-2">{t('Words must all appear in the title; “|” separates alternatives (e.g. “1080p linux|ubuntu”). Turn on regex for regular expressions.')}</p>
            {#each draft.rssRules as rule, i}
              <div class="rounded-[10px] border border-line bg-panel p-3 mb-2 space-y-2">
                <div class="flex items-center gap-2">
                  <input type="checkbox" class="toggle" bind:checked={rule.enabled} />
                  <input class="field !h-[28px] flex-1" bind:value={rule.name} />
                  <button class="icon-btn hover:!text-bad" title={t('Remove')} onclick={() => (draft!.rssRules = draft!.rssRules.filter((_, j) => j !== i))}><Icon name="trash" size={13} /></button>
                </div>
                <div class="grid grid-cols-2 gap-2">
                  <label class="flex flex-col gap-1"><span class="label">{t('Must contain')}</span><input class="field !h-[28px]" bind:value={rule.mustContain} /></label>
                  <label class="flex flex-col gap-1"><span class="label">{t('Must not contain')}</span><input class="field !h-[28px]" bind:value={rule.mustNotContain} /></label>
                </div>
                <div class="flex items-center gap-4 text-[12px] flex-wrap">
                  <label class="flex items-center gap-1.5"><input type="checkbox" class="check" bind:checked={rule.useRegex} />{t('Regular expressions')}</label>
                  <label class="flex items-center gap-1.5"><input type="checkbox" class="check" bind:checked={rule.startPaused} />{t('Add paused')}</label>
                </div>
                {#if draft.rssFeeds.length}
                  <div class="flex items-center gap-2 flex-wrap text-[11.5px]">
                    <span class="label">{t('Feeds')}:</span>
                    {#each draft.rssFeeds as feed}
                      <label class="flex items-center gap-1"><input type="checkbox" class="check" checked={rule.feeds.length === 0 || rule.feeds.includes(feed.url)} onchange={() => toggleRuleFeed(i, feed.url)} />{feed.name || feed.url}</label>
                    {/each}
                  </div>
                {/if}
                <div class="flex gap-2">
                  <input class="field !h-[28px]" placeholder={t('Save to (default folder when empty)')} bind:value={rule.saveDir} />
                  <button class="btn !h-[28px]" onclick={() => browseInto((p) => (rule.saveDir = p), rule.saveDir)}><Icon name="folderOpen" size={12} /></button>
                </div>
              </div>
            {/each}
            <button class="btn" onclick={addRule}><Icon name="plus" size={12} />{t('Add rule')}</button>
          {:else if current.id === 'automation'}
            {@render toggle('Open the folder when a download finishes', 'Shows the finished file in its folder. When many downloads finish together, each folder opens only once a minute.', draft.openFolderWhenDone, (v) => (draft!.openFolderWhenDone = v))}
            {@render row('Remove finished downloads from the list after (days)', 'Keeps the list short. Only the entries are removed, the files stay. Seeding torrents are kept. 0 = never.')}
            <input type="number" min="0" max="3650" class="field !w-[100px] mt-1 mb-2" bind:value={draft.autoClearDays} />
            {@render row('Run a program when a download finishes', 'Placeholders: {file}, {dir}, {name}, {url}. Example: an antivirus scan or a script that moves the file.')}
            <input class="field mono mt-1" placeholder={'"C:\\Tools\\scan.exe" {file}'} bind:value={draft.onCompleteCommand} />
            {@render row('When all downloads have finished')}
            <select class="field !w-[260px] mt-1" bind:value={draft.whenDone}>
              <option value="nothing">{t('Do nothing')}</option>
              <option value="sleep">{t('Put the computer to sleep')}</option>
              <option value="shutdown">{t('Shut down the computer')}</option>
              <option value="quit">{t('Quit Multron Download Manager')}</option>
            </select>
            <p class="text-[11px] text-muted mt-2">{t('Runs once, then switches back to “Do nothing”. Windows waits 60 seconds before shutting down (cancel with “shutdown /a”). Seeding torrents do not count as downloads.')}</p>
          {:else if current.id === 'trackers'}
            {@render row('Extra trackers', 'Announced for every torrent, one URL per line.')}
            <textarea class="field mt-1" rows="10" bind:value={trackersText} placeholder="udp://tracker.opentrackr.org:1337/announce"></textarea>
            <button class="btn mt-2" onclick={addPublicTrackers}><Icon name="plus" size={12} />{t('Add public tracker list')}</button>
            {@render toggle('Add public trackers automatically', 'On by default. Torrents and magnet links that are not private also announce to a list of well-known public trackers (refreshed once a day), so they find more peers and download faster. Private torrents are never changed.', draft.torrentPublicTrackers, (v) => (draft!.torrentPublicTrackers = v))}
            {@render toggle('Scrape trackers for seeds / leechers', 'Shown in the Sources tab of each torrent.', draft.torrentScrape, (v) => (draft!.torrentScrape = v))}
            {@render toggle('Disable trackers (DHT/PEX only)', '', draft.torrentDisableTrackers, (v) => (draft!.torrentDisableTrackers = v))}
          {:else if current.id === 'seeding'}
            {@render toggle('Keep seeding after a torrent completes', '', draft.seedAfterComplete, (v) => (draft!.seedAfterComplete = v))}
            {@render row('Stop seeding at share ratio', '0 = seed until removed.')}
            <input type="number" min="0" step="0.1" class="field !w-[120px] mt-1" bind:value={draft.seedRatio} />
            {@render row('Stop seeding after (minutes)', '0 = no time limit.')}
            <input type="number" min="0" class="field !w-[120px] mt-1" bind:value={draft.seedTimeLimit} />
            {@render row('When a limit is reached')}
            <select class="field !w-[260px] mt-1" bind:value={draft.seedLimitAction}>
              <option value="pause">{t('Pause the torrent')}</option>
              <option value="remove">{t('Remove the torrent, keep the files')}</option>
              <option value="delete">{t('Remove the torrent and its files')}</option>
            </select>
            {@render row('Torrent download limit (KB/s)', '0 = unlimited.')}
            <input type="number" min="0" class="field !w-[160px] mt-1" bind:value={draft.torrentDownloadLimit} />
            {@render row('Torrent upload limit (KB/s)', '0 = unlimited.')}
            <input type="number" min="0" class="field !w-[160px] mt-1" bind:value={draft.torrentUploadLimit} />
          {:else if current.id === 'privacy'}
            {@render row('SOCKS5 proxy for peer connections', 'socks5://[user:pass@]host:port — e.g. a VPN provider or Tor. All outgoing peer connections go through it.')}
            <input class="field mono mt-1" placeholder="socks5://127.0.0.1:9050" bind:value={draft.torrentProxy} />
            {@render row('Bind to network interface', 'Only use this adapter (e.g. "tun0", "wg0", "utun3") so traffic stops if the VPN drops. Linux and macOS.')}
            <input class="field mono mt-1 !w-[260px]" placeholder="wg0" bind:value={draft.torrentBindInterface} />
            {@render toggle('IPv4 only', '', draft.torrentIpv4Only, (v) => (draft!.torrentIpv4Only = v))}
            {@render toggle('Hide client name and version', 'Peers see a generic client name instead of "Multron Download Manager x.y".', draft.torrentHideClient, (v) => (draft!.torrentHideClient = v))}
            {@render toggle('Local Service Discovery', 'Turn off to stop announcing torrents on your LAN.', draft.torrentLsd, (v) => (draft!.torrentLsd = v))}
            {@render toggle('DHT', 'Turn off to only get peers from trackers.', draft.torrentDht, (v) => (draft!.torrentDht = v))}
            {@render toggle('UPnP port forwarding', '', draft.torrentUpnp, (v) => (draft!.torrentUpnp = v))}
            <p class="text-[11px] text-muted mt-3">{t('Private torrents never use DHT or LSD. Combine with IP Filters to block known bad ranges.')}</p>
          {:else if current.id === 'ipfilters'}
            {@render toggle(t('Enable'), '', draft.ipFilterEnabled, (v) => (draft!.ipFilterEnabled = v))}
            {@render toggle('ALLOW these ranges (default is DENY)', 'When on, only peers inside the listed ranges are allowed.', draft.ipFilterAllowMode, (v) => (draft!.ipFilterAllowMode = v))}
            {@render group('Auto Loading')}
            {@render row('IP filter file to autoload')}
            <div class="flex gap-2 mt-1">
              <input class="field" placeholder="C:\\lists\\ipfilter.dat · https://…" bind:value={draft.ipFilterSource} />
              <button class="btn" onclick={() => browseFilter(false)}><Icon name="folder" size={13} /></button>
              <button class="btn" onclick={loadFilterNow}>{t('Load Now')}</button>
            </div>
            {@render row('IPv6 filter file to autoload')}
            <div class="flex gap-2 mt-1">
              <input class="field" bind:value={draft.ipFilterSourceV6} />
              <button class="btn" onclick={() => browseFilter(true)}><Icon name="folder" size={13} /></button>
              <button class="btn" onclick={loadFilterNow}>{t('Load Now')}</button>
            </div>
            {@render group('Ready-made lists')}
            <p class="text-[11.5px] text-muted mb-2">{t('Well-known public blocklists. Use one to put it in the field above and load it now; it is refreshed after the reload period.')}</p>
            <div class="grid grid-cols-[repeat(auto-fill,minmax(250px,1fr))] gap-2 max-w-[760px]">
              {#each IP_LISTS as l (l.url)}
                {@const inUse = draft.ipFilterSource.trim() === l.url}
                <div class="rounded-[10px] border p-3 flex flex-col gap-1.5 {inUse ? 'border-accent/60 bg-accent/5' : 'border-line'}">
                  <div class="text-[12.5px] font-semibold flex items-center gap-1.5"><Icon name="shield" size={12} class="text-accent" />{l.name}</div>
                  <p class="text-[11.5px] text-muted leading-snug flex-1">{t(l.desc)}</p>
                  {#if inUse}
                    <span class="text-[11.5px] text-ok flex items-center gap-1"><Icon name="check" size={11} />{t('In use')}{filterStatus?.rules ? ` · ${t('{count} ranges loaded', { count: filterStatus.rules })}` : ''}</span>
                  {:else}
                    <button class="btn !h-[28px] self-start" onclick={() => useIpList(l.url)}><Icon name="down" size={12} />{t('Use this list')}</button>
                  {/if}
                </div>
              {/each}
            </div>
            {@render row('Auto reload period [days]')}
            <input type="number" min="1" max="365" class="field !w-[100px] mt-1" bind:value={draft.ipFilterReloadDays} />
            <p class="text-[11px] text-muted mt-2 leading-relaxed">
              {t('Supports DAT (eMule), P2P (PeerGuardian) and CIDR lists. Files can be local or URLs, plain, gzipped or zipped. URLs are downloaded again after the reload period.')}
            </p>
            {@render group('Manual ranges')}
            <p class="text-[11.5px] text-muted mb-1.5">{t('Your own ranges, one per line, added to the list above. Empty by default. Formats: a single address (203.0.113.7), a range with an optional name (Bad peers:10.0.0.0-10.0.0.255) or CIDR (192.168.5.0/24).')}</p>
            <textarea class="field mono text-[12px]" rows="5" placeholder={t('One range per line…')} bind:value={draft.ipFilterManual}></textarea>
            <div class="text-[11.5px] mt-2 {filterStatus?.error ? 'text-bad' : 'text-muted'}">
              {#if filterStatus?.error}{filterStatus.error}{:else if filterStatus}{t('{count} ranges loaded', { count: filterStatus.rules })}{filterStatus.loadedAt ? ` · ${date(filterStatus.loadedAt, i18n.lang)}` : ''}{/if}
            </div>
          {:else if current.id === 'components'}
            <p class="text-[12px] text-muted mb-3">{t('Built-in components and external tools. Unchecked components are not loaded at startup.')}</p>
            <table class="grid-table border border-line rounded">
              <thead><tr><th>{t('Load at startup')}</th><th>{t('Type')}</th><th>{t('Name')}</th><th>{t('Version')}</th><th>{t('Status')}</th></tr></thead>
              <tbody>
                <tr><td><input type="checkbox" class="check" bind:checked={draft.torrentEnabled} /></td><td>{t('Built in')}</td><td>{t('BitTorrent engine (librqbit)')}</td><td>9</td><td>{app.stats.torrent ? '● Running' : 'Idle'}</td></tr>
                <tr><td><input type="checkbox" class="check" bind:checked={draft.mediaEnabled} /></td><td>{t('External')}</td><td>{t('Media downloader (yt-dlp)')}</td><td>{tools?.ytdlpVersion ?? '—'}</td><td>{tools?.ytdlpInstalled ? 'Installed' : 'On demand'}</td></tr>
                <tr><td><input type="checkbox" class="check" checked={tools?.ffmpegAvailable ?? false} disabled /></td><td>{t('External')}</td><td>{t('FFmpeg (merging / audio)')}</td><td>—</td><td>{tools?.ffmpegAvailable ? 'Available' : 'Missing'}</td></tr>
                <tr><td><input type="checkbox" class="check" checked={tools?.denoAvailable ?? false} disabled /></td><td>{t('External')}</td><td>{t('Deno (YouTube challenges)')}</td><td>{tools?.denoVersion ?? '—'}</td><td>{tools?.denoAvailable ? 'Available' : 'On demand'}</td></tr>
                <tr><td><input type="checkbox" class="check" bind:checked={draft.bridgeEnabled} /></td><td>{t('Built in')}</td><td>{t('Browser extension bridge')}</td><td>{app.version}</td><td>{bridge?.running ? '● Listening' : 'Off'}</td></tr>
                <tr><td><input type="checkbox" class="check" bind:checked={draft.torrentScrape} /></td><td>{t('Built in')}</td><td>{t('Tracker scraper')}</td><td>{app.version}</td><td>{draft.torrentScrape ? 'On' : 'Off'}</td></tr>
                <tr><td><input type="checkbox" class="check" bind:checked={draft.torrentUpnp} /></td><td>{t('Built in')}</td><td>{t('Universal Plug and Play (UPnP)')}</td><td>—</td><td>{draft.torrentUpnp ? 'On' : 'Off'}</td></tr>
                <tr><td><input type="checkbox" class="check" bind:checked={draft.torrentUtp} /></td><td>{t('Built in')}</td><td>{t('uTP transport')}</td><td>—</td><td>{draft.torrentUtp ? 'On' : 'Off'}</td></tr>
                <tr><td><input type="checkbox" class="check" bind:checked={draft.torrentDht} /></td><td>{t('Built in')}</td><td>{t('Mainline DHT')}</td><td>—</td><td>{app.stats.torrent?.dhtNodes != null ? `${app.stats.torrent.dhtNodes} nodes` : draft.torrentDht ? 'On' : 'Off'}</td></tr>
                <tr><td><input type="checkbox" class="check" bind:checked={draft.torrentLsd} /></td><td>{t('Built in')}</td><td>{t('Local Service Discovery')}</td><td>—</td><td>{draft.torrentLsd ? 'On' : 'Off'}</td></tr>
                <tr><td><input type="checkbox" class="check" bind:checked={draft.ipFilterEnabled} /></td><td>{t('Built in')}</td><td>{t('IP filter')}</td><td>—</td><td>{filterStatus?.rules ? `${filterStatus.rules} ranges` : 'Empty'}</td></tr>
              </tbody>
            </table>
          {:else if current.id === 'cloud'}
            <h4 class="text-[12px] font-semibold text-white/90 mt-1 mb-1 pb-1 border-b border-line flex items-center gap-2">{t('Multron Cloud scan')}<span class="px-1.5 rounded bg-warn/15 text-warn text-[10px] font-semibold uppercase tracking-wide">{t('Testing')}</span></h4>
            <div class="rounded-[12px] border border-accent/50 bg-accent/10 p-3.5 mb-3 shadow-lg">
              <div class="text-[13.5px] font-semibold">{t('Cloud-based malware protection')}</div>
              <p class="text-[11.5px] text-muted leading-snug mt-0.5 mb-1.5">{t('Multron Cloud checks your downloads for viruses and other malware in the cloud, separately from your antivirus. With real-time scanning, every download is checked the moment it finishes.')}</p>
              <div class={draft.cloudScanAccepted ? '' : 'opacity-45 pointer-events-none'}>
                {@render toggle('Scan finished downloads with Multron Cloud', 'On by default. Every download of the file types below is scanned the moment it finishes, without pressing the button. The agreement is asked when Multron Download Manager starts. The result is shown on the download as “Multron Cloud”.', draft.cloudScanAccepted && draft.cloudScan, (v) => (draft!.cloudScan = v))}
              </div>
              <label class="flex items-start gap-2 py-[5px] text-[12px] cursor-pointer">
                <input type="checkbox" class="check mt-[2px]" checked={draft.cloudScanAccepted} onchange={(e) => { draft!.cloudScanAccepted = e.currentTarget.checked; if (!e.currentTarget.checked) draft!.cloudScan = false; }} />
                <span>{t('I have read and accept the Cloud Malware Scan License Agreement.')}</span>
              </label>
              <button class="btn" onclick={() => api.openUrl('https://github.com/Multron-Community/MultronWcleaner/blob/beta/MALWARE_SCAN_EULA.md')}><Icon name="globe" size={13} />{t('Read the agreement')}</button>
            </div>
            {@render toggle('Show the Scan for malware button', 'On by default. Finished downloads get a button that scans them with Multron Cloud when you press it. The agreement is asked only the first time.', draft.cloudScanButton, (v) => (draft!.cloudScanButton = v))}
            {@render toggle('Show the Scan all button', 'On by default. A button in the toolbar scans every finished download in one window and lists the threats it finds.', draft.cloudScanAllButton, (v) => (draft!.cloudScanAllButton = v))}
            <div class={draft.cloudScanButton ? '' : 'opacity-45 pointer-events-none'}>
              {@render row('Upload limit for the button (MB)', 'Default 100 MB, the most Multron Cloud accepts. Larger files are only checked by their hash.')}
              <input type="number" min="1" max="100" class="field !w-[100px] mt-1" bind:value={draft.cloudScanManualLimitMb} />
            </div>
            {@render group('Cloud connection')}
            {@render toggle('Compress uploads (faster on slow connections)', 'On by default. Unknown files are compressed with Brotli before they are uploaded. Turn it off on a very fast network to save CPU time.', draft.cloudCompressUploads, (v) => (draft!.cloudCompressUploads = v))}
            {@render toggle('Remember file fingerprints', 'On by default. Files that have not changed since the last scan are not hashed again, so repeated scans start sooner.', draft.cloudFingerprintCache, (v) => (draft!.cloudFingerprintCache = v))}
            {@render row('Connections', 'Parallel connections to the cloud server (1 to 8, default 4); more connections upload more unknown files at once.')}
            <input type="number" min="1" max="8" class="field !w-[100px] mt-1 mb-2" bind:value={draft.cloudConnections} />
            <div class={draft.cloudScanAccepted && draft.cloudScan ? '' : 'opacity-45 pointer-events-none'}>
              {@render row('Upload limit (MB)', 'Larger files are only checked by their hash and are never uploaded. Default 15 MB, at most 100 MB.')}
              <input type="number" min="1" max="100" class="field !w-[100px] mt-1" bind:value={draft.cloudScanLimitMb} />
              {@render row('File types for real-time scan', 'Only these files are scanned on their own. Programs and scripts are selected by default; the Scan for malware button always checks every file.')}
              <label class="flex items-center gap-1.5 text-[12px] cursor-pointer mt-2">
                <input type="checkbox" class="check" checked={draft.cloudScanExtensions.includes('*')} onchange={(e) => toggleExtension('*', e.currentTarget.checked)} />{t('All files')}
              </label>
              <div class={draft.cloudScanExtensions.includes('*') ? 'opacity-45 pointer-events-none' : ''}>
              <div class="grid grid-cols-4 gap-x-3 gap-y-1 mt-2 max-w-[460px]">
                {#each cloudExtensionChoices as ext}
                  <label class="flex items-center gap-1.5 text-[12px] cursor-pointer mono">
                    <input type="checkbox" class="check" checked={draft.cloudScanExtensions.includes(ext)} onchange={(e) => toggleExtension(ext, e.currentTarget.checked)} />.{ext}
                  </label>
                {/each}
              </div>
              {#if draft.cloudScanExtensions.some((x) => x !== '*' && !cloudExtensionChoices.includes(x))}
                <div class="flex flex-wrap gap-1.5 mt-2">
                  {#each draft.cloudScanExtensions.filter((x) => x !== '*' && !cloudExtensionChoices.includes(x)) as ext}
                    <button class="h-[22px] px-2 rounded-full border border-line text-[11px] mono flex items-center gap-1 hover:border-bad/60 cursor-pointer" title={t('Remove')} onclick={() => toggleExtension(ext, false)}>.{ext}<Icon name="x" size={10} /></button>
                  {/each}
                </div>
              {/if}
              <input class="field !w-[260px] mt-2" placeholder={t('Other types, for example: iso, img, pdf')} bind:value={customExtensions} />
              </div>
              <div class="flex gap-2 mt-2">
                <button class="btn" onclick={() => (draft!.cloudScanExtensions = [...cloudExtensionDefaults])}>{t('Programs and scripts only')}</button>
              </div>
            </div>
            {@render group('Protection')}
            {@render toggle('Move threats to quarantine', 'On by default. Files that Multron Cloud finds malicious are moved to a locked quarantine folder and renamed, so they cannot be started by accident. You can restore or delete them below.', draft.cloudQuarantine, (v) => (draft!.cloudQuarantine = v))}
            {@render toggle('Block opening threats', 'On by default. Files marked as a threat or suspicious do not open from Multron Download Manager until you confirm it in a warning.', draft.cloudBlockThreats, (v) => (draft!.cloudBlockThreats = v))}
            {@render toggle('Check again before opening', 'On by default. When you open a download that was never scanned or whose last scan is older than the days below, Multron Cloud checks it first and the file opens when it is clean.', draft.cloudRescanOnOpen, (v) => (draft!.cloudRescanOnOpen = v))}
            <div class="grid grid-cols-[minmax(0,1fr)_120px] gap-x-4 items-center max-w-[560px] mt-1 pl-[42px] {draft.cloudRescanOnOpen ? '' : 'opacity-45 pointer-events-none'}">
              <span class="text-[12.5px]">{t('Scan again when the last scan is older than (days)')}</span>
              <input type="number" min="1" max="365" class="field" bind:value={draft.cloudRescanDays} />
            </div>
            {@render toggle('Keep a scan history', 'On by default. Every finished scan is listed below with its date, result and details.', draft.cloudHistory, (v) => (draft!.cloudHistory = v))}
            <div class="flex items-center gap-3 pl-[42px] mt-1 {draft.cloudHistory ? '' : 'opacity-45 pointer-events-none'}">
              <span class="text-[12.5px]">{t('Delete history older than')}</span>
              <select class="field !w-auto !h-[30px] !text-[12px]" bind:value={draft.cloudHistoryDays}>
                <option value={7}>{t('1 week')}</option>
                <option value={30}>{t('1 month')}</option>
                <option value={90}>{t('3 months')}</option>
                <option value={365}>{t('1 year')}</option>
                <option value={0}>{t('Never')}</option>
              </select>
            </div>

            <div class="flex gap-1 border-b border-line mt-5">
              <button type="button" class="tab flex items-center gap-1.5 {protectionTab === 'quarantine' ? 'tab-active' : ''}" onclick={() => (protectionTab = 'quarantine')}><Icon name="lock" size={12} />{t('Quarantine')} <span class="text-dim">{quarantine.length}</span></button>
              <button type="button" class="tab flex items-center gap-1.5 {protectionTab === 'history' ? 'tab-active' : ''}" onclick={() => (protectionTab = 'history')}><Icon name="clock" size={12} />{t('Scan history')} <span class="text-dim">{history.length}</span></button>
              <div class="flex-1"></div>
              <button type="button" class="icon-btn" title={t('Refresh')} onclick={loadProtection}><Icon name="refresh" size={12} /></button>
            </div>
            {#if protectionTab === 'quarantine'}
              {#each quarantine as q (q.id)}
                <div class="flex items-center gap-3 py-2.5 border-b border-line/50">
                  <div class="w-[32px] h-[32px] rounded-[8px] bg-bad/15 text-bad flex items-center justify-center flex-shrink-0"><Icon name="alert" size={15} /></div>
                  <div class="min-w-0 flex-1">
                    <div class="text-[12.5px] font-medium truncate" title={q.original}>{q.name}</div>
                    <div class="text-[11px] text-muted truncate" title={q.detail}>{q.detail} · {bytes(q.size)} · {new Date(q.at).toLocaleString()}</div>
                  </div>
                  <button class="btn !h-[28px]" onclick={() => restoreItem(q)}>{t('Restore')}</button>
                  <button class="btn !h-[28px] hover:!text-bad" onclick={() => deleteItem(q)}><Icon name="trash" size={12} />{t('Delete')}</button>
                </div>
              {:else}
                <p class="py-6 text-center text-[12px] text-muted">{t('The quarantine is empty.')}</p>
              {/each}
            {:else}
              <div class="flex items-center gap-1.5 py-2.5 flex-wrap">
                {#each [['all', 'All'], ['threat', 'Threats'], ['clean', 'Clean'], ['other', 'Not finished']] as [id, label] (id)}
                  <button class="chip {historyFilter === id ? 'chip-on' : ''}" onclick={() => (historyFilter = id as typeof historyFilter)}>{t(label)}</button>
                {/each}
                <div class="flex-1"></div>
                {#if history.length}<button class="btn !h-[26px] !text-[11.5px]" onclick={clearHistory}>{t('Clear history')}</button>{/if}
              </div>
              {#each shownHistory as h (h.id)}
                <div class="grid grid-cols-[minmax(0,1fr)_110px_150px] gap-3 items-center py-2 border-b border-line/40 text-[12px]">
                  <div class="min-w-0">
                    <div class="truncate font-medium" title={h.name}>{h.name}</div>
                    <div class="text-[11px] text-muted truncate" title={h.detail}>{h.detail}</div>
                  </div>
                  <span class="flex items-center gap-1 {stateTone[h.state] ?? 'text-muted'}">
                    <Icon name={h.state === 'threat' || h.state === 'suspicious' ? 'alert' : 'shield'} size={11} />{t(stateLabel[h.state] ?? 'Not finished')}{h.quarantined ? ' · ' + t('Quarantined') : ''}
                  </span>
                  <span class="text-[11px] text-dim text-right">{new Date(h.at).toLocaleString()}</span>
                </div>
              {:else}
                <p class="py-6 text-center text-[12px] text-muted">{t('No scans yet.')}</p>
              {/each}
            {/if}
          {:else if current.id === 'anonymity'}
            {@render group('Tor')}
            <div class="rounded-[12px] border border-[#7d4cdb]/40 bg-[#7d4cdb]/10 p-3.5 mb-2 text-[11.5px] text-muted leading-relaxed space-y-1.5">
              <p>{t('Tor sends downloads through three relays around the world, so the site you download from sees a Tor address instead of yours, and your internet provider only sees that you use Tor, not what you download.')}</p>
              <p>{t('Tor is slower than a direct connection, and some sites block it. Multron Download Manager does not include Tor: start Tor Browser or the Tor service, then turn this on.')}</p>
            </div>
            {@render toggle('Download through Tor', 'Off by default. Links and videos are downloaded through Tor, including the name lookups, so nothing leaks around it. If Tor is not running, downloads stop instead of going out directly.', draft.torEnabled, (v) => (draft!.torEnabled = v))}
            <div class="flex gap-2 max-w-[560px] mt-1">
              <input class="field mono flex-1 min-w-0" bind:value={draft.torProxy} placeholder="socks5h://127.0.0.1:9050" spellcheck="false" />
              <button class="btn" type="button" disabled={torState.busy} onclick={detectTor}>{t('Find Tor')}</button>
              <button class="btn" type="button" disabled={torState.busy} onclick={testTor}>{t('Test')}</button>
            </div>
            {#if torState.text}
              <p class="text-[11.5px] mt-1.5 {torState.tone === 'ok' ? 'text-ok' : torState.tone === 'bad' ? 'text-bad' : 'text-muted'}">{torState.text}</p>
            {/if}
            {@render toggle('Block torrents while Tor is on', 'On by default and recommended. BitTorrent connects to other people directly and over UDP, which Tor cannot carry, so torrents would reveal your real address even with Tor on. The Tor Project asks people not to use BitTorrent over Tor.', draft.torBlockTorrents, (v) => (draft!.torBlockTorrents = v))}

            {@render group('Download history')}
            {@render toggle('Remember finished downloads', 'On by default. Finished downloads are kept in the download history, also after they are removed from the list.', draft.historyEnabled, (v) => (draft!.historyEnabled = v))}
            <div class="pl-[42px] {draft.historyEnabled ? '' : 'opacity-45 pointer-events-none'}">
              {@render toggle('Warn about links downloaded before', 'On by default. Adding a link that is already in the history asks before downloading it again.', draft.historyWarn, (v) => (draft!.historyWarn = v))}
              {@render toggle('Clear the download history on exit', 'The history is kept only while the app is open.', draft.privacyClearHistoryOnExit, (v) => (draft!.privacyClearHistoryOnExit = v))}
            </div>
            {@render group('Leave fewer traces')}
            {@render toggle('Ask sites not to track or sell your data', 'Sends the Global Privacy Control (Sec-GPC) and Do Not Track signals with every download. Sites decide themselves whether to follow them.', draft.privacyGpc, (v) => (draft!.privacyGpc = v))}
            {@render toggle('Hide file names in notifications', 'Notices only say that a download started, finished or failed, without its name. Useful on shared or recorded screens.', draft.privacyHideNames, (v) => (draft!.privacyHideNames = v))}
            {@render toggle('Do not send the page address (Referer)', 'Sites are not told which page a download link came from. A few hosts need it and refuse the download without it.', draft.privacyNoReferrer, (v) => (draft!.privacyNoReferrer = v))}
            {@render toggle('Remove finished downloads from the list on exit', 'The list forgets completed downloads when Multron Download Manager closes. The files themselves stay where they are.', draft.privacyClearFinishedOnExit, (v) => (draft!.privacyClearFinishedOnExit = v))}
            {@render toggle('Delete saved site cookies on exit', 'Cookies that the browser extension handed over for protected downloads are deleted when Multron Download Manager closes.', draft.privacyClearCookiesOnExit, (v) => (draft!.privacyClearCookiesOnExit = v))}
            {@render toggle('Do not remember LinkGrabber links', 'Links collected in LinkGrabber are kept only while the app is open.', draft.privacyForgetLinkgrabber, (v) => (draft!.privacyForgetLinkgrabber = v))}
            {@render toggle('Keep a scan history', 'On by default. Every finished scan is listed in Multron Cloud with its date, result and details.', draft.cloudHistory, (v) => (draft!.cloudHistory = v))}
          {:else if current.id === 'virustotal'}
            <p class="text-[11.5px] text-muted leading-relaxed mb-1">{t('Check a finished download with more than 70 antivirus engines on VirusTotal. Only the file’s SHA-256 fingerprint is sent first. If VirusTotal does not know the file yet, you are asked before it is uploaded. Get a free API key on virustotal.com (your profile → API key); it allows 4 checks a minute and 500 a day.')}</p>
            <div class="flex gap-2 max-w-[560px]">
              <input class="field mono flex-1 min-w-0" type={showVtKey ? 'text' : 'password'} autocomplete="off" spellcheck="false" placeholder={t('VirusTotal API key')} bind:value={draft.virustotalKey} />
              <button class="btn" type="button" onclick={() => (showVtKey = !showVtKey)}>{showVtKey ? t('Hide') : t('Show')}</button>
            </div>
            <div class="grid grid-cols-2 gap-2 max-w-[620px] mt-3">
              <div class="rounded-[10px] border p-3 {draft.virustotalKey ? 'border-line' : 'border-accent/50 bg-accent/5'}">
                <div class="text-[12.5px] font-semibold flex items-center gap-1.5"><Icon name="globe" size={12} />{t('Without a key')}</div>
                <p class="text-[11.5px] text-muted mt-1 leading-snug">{t('The button opens the file’s page on virustotal.com in your browser. Only its fingerprint is sent; if VirusTotal knows the file, the results show right away.')}</p>
              </div>
              <div class="rounded-[10px] border p-3 {draft.virustotalKey ? 'border-accent/50 bg-accent/5' : 'border-line'}">
                <div class="text-[12.5px] font-semibold flex items-center gap-1.5"><Icon name="lock" size={12} />{t('With a key')}</div>
                <p class="text-[11.5px] text-muted mt-1 leading-snug">{t('The results show inside Multron Download Manager, and a file VirusTotal does not know yet can be uploaded after you agree.')}</p>
              </div>
            </div>
            {@render toggle('Show the VirusTotal button', 'On by default. Finished downloads get a VirusTotal button next to the scan button, with or without a key.', draft.virustotalButton, (v) => (draft!.virustotalButton = v))}
          {:else if current.id === 'captcha'}
            <div class="rounded-[12px] border border-line bg-raised/60 p-4 flex gap-3 items-start">
              <div class="w-[38px] h-[38px] rounded-[10px] bg-accent/15 text-accent flex items-center justify-center flex-shrink-0"><Icon name="shield" size={18} /></div>
              <div class="space-y-1">
                <div class="text-[13px] font-semibold">{t('You solve CAPTCHAs, everything else is automatic')}</div>
                <p class="text-[11.5px] text-muted leading-relaxed">{t('Some sites show a CAPTCHA before they give the file. Multron Download Manager recognizes it, opens the page for you, lets you know and tries the download again by itself once you have solved it. It never solves CAPTCHAs by itself and never sends them to solving services.')}</p>
              </div>
            </div>

            {@render group('When a site asks for a CAPTCHA')}
            {@render toggle('Open the page in my browser automatically', 'On by default. The page opens in your default browser right away. Solve it there; the browser extension then hands the download over.', draft.captchaOpenBrowser, (v) => (draft!.captchaOpenBrowser = v))}
            {@render toggle('Play a sound', 'On by default. A short chime plays when a new CAPTCHA is waiting.', draft.captchaSound, (v) => (draft!.captchaSound = v))}
            {@render toggle('Show a notice', 'On by default. A notice appears in the corner of the screen, even when the window is in the background.', draft.captchaNotice, (v) => (draft!.captchaNotice = v))}

            {@render group('After solving')}
            {@render toggle('Try the download again automatically', 'On by default. While the CAPTCHA is waiting, the download is retried at the interval below, so it continues by itself once the site lets it through.', draft.captchaAutoRetry, (v) => (draft!.captchaAutoRetry = v))}
            <div class="grid grid-cols-[minmax(0,1fr)_120px] gap-x-4 gap-y-3 items-center max-w-[560px] mt-2 {draft.captchaAutoRetry ? '' : 'opacity-45 pointer-events-none'}">
              <span class="text-[12.5px]">{t('Retry every (seconds)')}</span>
              <input type="number" min="10" max="600" class="field" bind:value={draft.captchaRetrySecs} />
            </div>
            <div class="grid grid-cols-[minmax(0,1fr)_120px] gap-x-4 gap-y-3 items-center max-w-[560px] mt-3">
              <span class="flex flex-col gap-[2px]"><span class="text-[12.5px]">{t('Give up after (seconds)')}</span><span class="text-[11px] text-muted">{t('A CAPTCHA that was not solved by then leaves the waiting list.')}</span></span>
              <input type="number" min="30" max="3600" class="field" bind:value={draft.captchaTimeoutSecs} />
            </div>

            {#if app.captchas.length}
              {@render group('Waiting now')}
              {#each app.captchas as c (c.id)}
                <div class="flex items-center gap-3 py-2 border-b border-line/50 text-[12px]">
                  <Icon name="lock" size={13} class="text-warn" />
                  <span class="flex-1 min-w-0"><span class="font-medium">{c.site}</span> <span class="text-muted">· {c.kind}</span></span>
                  <button class="btn !h-[26px]" onclick={() => api.captchaOpen(c.id).catch(() => {})}>{t('Open in browser')}</button>
                  <button class="icon-btn" title={t('Dismiss')} onclick={() => api.captchaDismiss(c.id)}><Icon name="x" size={12} /></button>
                </div>
              {/each}
            {/if}
          {:else if current.id === 'security'}
            <div class="security-level rounded-[12px] border border-line bg-raised p-4 mb-2" data-level={securityLevel}>
              <div class="flex flex-col gap-0.5 mb-3">
                <span class="text-[13px] font-semibold">{t('Security level')}</span>
                <span class="text-[11px] text-muted">{t('Sets all the options below at once. Changing one of them yourself makes it Custom.')}</span>
              </div>
              <div class="flex gap-5 items-stretch">
                <div class="flex flex-col items-center w-[118px] flex-shrink-0">
                  <span class="text-[11px] font-medium text-muted text-center leading-tight">{t('More protection')}</span>
                  <div
                    class="sec-slider relative w-[56px] h-[164px] my-2 cursor-pointer outline-none rounded-[8px]"
                    role="slider"
                    tabindex="0"
                    aria-label={t('Security level')}
                    aria-orientation="vertical"
                    aria-valuemin={0}
                    aria-valuemax={SECURITY_LEVELS.length - 1}
                    aria-valuenow={securityLevel === 'custom' ? undefined : securityStep}
                    aria-valuetext={securityLevel === 'custom' ? t('Custom') : t(SECURITY_LEVELS[securityStep].label)}
                    onkeydown={securityKey}
                    onpointerdown={securityPointer}
                    onpointermove={(e) => e.buttons === 1 && securityPointer(e)}
                  >
                    <span class="sec-track absolute left-1/2 top-[10px] bottom-[10px] w-[4px] -translate-x-1/2 rounded-full"></span>
                    {#each SECURITY_LEVELS as l, i (l.id)}
                      <span class="sec-notch absolute left-1/2 -translate-x-1/2 w-[26px] h-[2px] rounded-full {securityLevel !== 'custom' && i <= securityStep ? 'on' : ''}" style="top: calc(10px + {(SECURITY_LEVELS.length - 1 - i) / (SECURITY_LEVELS.length - 1)} * (100% - 20px) - 1px)"></span>
                    {/each}
                    {#if securityLevel !== 'custom'}
                      <span class="sec-thumb absolute left-1/2 w-[34px] h-[14px] rounded-[5px] -translate-x-1/2 -translate-y-1/2" style="top: calc(10px + {(SECURITY_LEVELS.length - 1 - securityStep) / (SECURITY_LEVELS.length - 1)} * (100% - 20px))"></span>
                    {/if}
                  </div>
                  <span class="text-[11px] font-medium text-muted text-center leading-tight">{t('Fewer questions')}</span>
                </div>
                <div class="sec-info flex-1 rounded-[10px] border border-line p-4 min-w-0">
                  <div class="text-[14px] font-semibold">{securityLevel === 'custom' ? t('Custom') : t(SECURITY_LEVELS[securityStep].label)}</div>
                  <p class="text-[12px] text-muted leading-relaxed mt-1.5">
                    {#if securityLevel === 'custom'}{t('Your own mix of the options below. Pick a level to go back to a ready-made set.')}{:else}{t(SECURITY_LEVELS[securityStep].desc)}{#if SECURITY_EXTRA[securityLevel]}{' ' + t(SECURITY_EXTRA[securityLevel])}{/if}{/if}
                  </p>
                  <p class="text-[11px] text-dim leading-relaxed mt-1.5">
                    {t('Every level also keeps app and yt-dlp updates, checksum checks from the download page and Multron Cloud threat blocking, quarantine and re-checks turned on.')}
                  </p>
                </div>
              </div>
            </div>
            {#if smartscreen && smartscreen !== 'unsupported'}
              {@const shield = app.platform === 'macos' ? 'Gatekeeper' : 'Windows SmartScreen'}
              {@render group(shield)}
              {#if smartscreen === 'missing'}
                <div class="rounded-[10px] border border-bad/40 bg-bad/10 p-3 text-[12px] flex gap-2 items-start mb-2">
                  <Icon name="alert" size={14} class="text-bad mt-[1px] flex-shrink-0" />
                  <span>{t('{name} was not found on this system. It may have been removed from the Windows image (for example a modified ISO), so these options are turned off.', { name: shield })}</span>
                </div>
              {:else if smartscreen === 'off'}
                <div class="rounded-[10px] border border-warn/40 bg-warn/10 p-3 text-[12px] flex gap-2 items-start mb-2">
                  <Icon name="alert" size={14} class="text-warn mt-[1px] flex-shrink-0" />
                  <span>{app.platform === 'macos' ? t('Gatekeeper is turned off on this Mac, so these options are turned off.') : t('SmartScreen is turned off (Windows Security › App & browser control › Reputation-based protection, or a group policy), so these options are turned off.')}</span>
                </div>
              {:else}
                <p class="text-[12px] text-ok flex items-center gap-1.5 mb-1"><Icon name="shield" size={13} />{t('{name} is active.', { name: shield })}</p>
              {/if}
              <div class={smartscreen === 'on' ? '' : 'opacity-45 pointer-events-none'}>
                {@render toggle(app.platform === 'macos' ? 'Mark downloaded files for Gatekeeper' : 'Mark downloaded files for SmartScreen', app.platform === 'macos' ? 'Adds the quarantine flag, so macOS checks apps and documents from the internet when they are opened.' : 'Adds the “downloaded from the internet” mark (Mark of the Web), so Windows SmartScreen and Office check the file when it is opened, like files downloaded with a browser.', smartscreen === 'on' && draft.markDownloads, (v) => (draft!.markDownloads = v))}
              </div>
            {/if}
            {@render group('Safer downloads')}
            {@render toggle('Only allow encrypted (HTTPS) links', 'Links that start with http:// are refused, because anyone on the network can read or swap what they download. Torrents are not affected.', draft.httpsOnly, (v) => (draft!.httpsOnly = v))}
            {@render toggle('Do not download programs from unencrypted links', 'Programs and scripts (.exe, .msi, .bat, …) from links that start with http:// are refused, because they could be swapped on the way. Other files still download.', draft.blockInsecurePrograms, (v) => (draft!.blockInsecurePrograms = v))}
            {@render toggle('Refuse links to your local network from the browser', 'Links to addresses in your own network, such as 192.168.x.x, localhost or a router, are refused when the browser extension or Click\'n\'Load sends them. A web page cannot make Multron Download Manager reach devices at home this way. Links you add yourself still work.', draft.blockPrivateLinks, (v) => (draft!.blockPrivateLinks = v))}
            {@render toggle('Leave program files in torrents unticked', 'On by default. When a torrent contains programs or scripts (.exe, .bat, .scr, …), they are not selected and a warning is shown. Fake videos and games often hide malware this way.', draft.skipTorrentPrograms, (v) => (draft!.skipTorrentPrograms = v))}
            {#if app.platform === 'windows'}
              {@render toggle('Check digital signatures of downloaded programs', 'On by default. Programs are checked with Windows Authenticode: signed ones show their publisher, unsigned or broken ones get a warning in the list and before they start.', draft.checkSignatures, (v) => (draft!.checkSignatures = v))}
            {/if}
            {@render toggle('Check the real file type', 'On by default. When a download is really a program although its name says otherwise, for example an .exe renamed to .pdf or .mp3, it is marked as a disguised program, you get an alert and it does not open without a warning.', draft.checkRealType, (v) => (draft!.checkRealType = v))}
            {@render toggle('Always ask before downloading programs from the browser', 'On by default. Programs and scripts that the browser extension sends (.exe, .msi, .bat, …) open the small download window first, so nothing runnable is saved without you seeing it.', draft.askForPrograms, (v) => (draft!.askForPrograms = v))}
            {@render toggle('Warn about disguised programs', 'On by default. Files whose name hides that they are programs, such as “invoice.pdf.exe” or names with hidden right-to-left characters, are marked in the list and always ask before they start.', draft.warnDisguisedNames, (v) => (draft!.warnDisguisedNames = v))}
            {@render group('Opening downloaded programs')}
            {@render toggle('Ask before starting downloaded programs', 'On by default, so a program you downloaded never starts by accident. Double-clicking a program (.exe, .msi, scripts, installers) or pressing its open button first shows what Multron Cloud and SmartScreen know about it. Other files open as usual.', draft.askBeforeStartingPrograms, (v) => (draft!.askBeforeStartingPrograms = v))}
            {@render toggle('Ask before opening any downloaded file', 'Every downloaded file, not only programs, asks once before it opens. Documents and media can also carry harmful content.', draft.askBeforeOpeningFiles, (v) => (draft!.askBeforeOpeningFiles = v))}
            {@render toggle('Require approval for browser extensions', 'Each browser extension must be approved once before it can add downloads. Web pages can never use the bridge.', draft.bridgeRequirePairing, (v) => (draft!.bridgeRequirePairing = v))}
            {@render group('Trusted extensions')}
            {#if app.settings?.trustedExtensions.length}
              {#each app.settings.trustedExtensions as o}
                <div class="flex items-center justify-between py-1 text-[12px]">
                  <span class="mono">{o}</span>
                  <button class="btn btn-danger !h-[26px]" onclick={() => revoke(o)}>{t('Revoke')}</button>
                </div>
              {/each}
            {:else}
              <p class="text-[12px] text-muted">{t('None yet.')}</p>
            {/if}
            {@render group('Certificates')}
            {@render toggle('Accept invalid TLS certificates', 'Not recommended: allows downloads from servers with expired or self-signed certificates, which also allows tampering. SHA-256 verification still protects you.', draft.tlsAcceptInvalidCerts, (v) => (draft!.tlsAcceptInvalidCerts = v))}
            {@render toggle('Block redirects from HTTPS to HTTP', 'Stops a download when a secure link redirects to an unencrypted address, where the file could be swapped on the way. Connections always use TLS 1.2 or newer.', draft.blockInsecureRedirects, (v) => (draft!.blockInsecureRedirects = v))}
            {@render group('Stored data')}
            <div class="flex items-center gap-2 mt-2">
              <button class="btn" onclick={() => app.run(async () => { const n = await api.clearCookies(); app.toast('success', t('{count} cookie files removed', { count: n })); })}>{t('Clear saved browser cookies')}</button>
              <button class="btn" onclick={() => api.openPath(app.dataDir)}><Icon name="folderOpen" size={13} />{t('Open data folder')}</button>
            </div>
          {:else if current.id === 'about'}
            <div class="flex items-center gap-4 mb-4">
              <img src="/logo.png" alt="" class="w-14 h-14 rounded-full" />
              <div>
                <div class="text-[15px] font-semibold">Multron Download Manager</div>
                <div class="text-[12px] text-muted">{t('Version')} {app.version} · {app.platform} · © 2026 Multron Community · {t('All rights reserved')}</div>
              </div>
            </div>
            <div class="flex items-start gap-2 p-3 rounded-[10px] border border-line bg-subtle/40 text-[12px] mb-2">
              <Icon name="refresh" size={14} class="text-accent mt-[1px] flex-shrink-0" />
              <span class="text-muted leading-relaxed">{t('Multron Download Manager updates itself when it starts: a new version is downloaded from GitHub, checked against the release’s SHA-256 list and installed before the app opens. Windows uses the setup, macOS the .dmg, Linux the AppImage or the .deb/.rpm package (asks for your password).')}</span>
            </div>
            {#if app.updateReady}
              <div class="rounded-[10px] border border-ok/40 bg-ok/10 p-3 my-3 flex items-center justify-between text-[12px]">
                <span>{t('Multron Download Manager {version} is downloaded and ready.', { version: app.updateReady.version })}</span>
                <button class="btn btn-primary" onclick={() => app.installUpdate()}>{t('Restart and update')}</button>
              </div>
            {/if}
            {#if updateProgress}
              {@const p = updateProgress}
              {@const pct = p.total ? percent(p.downloaded, p.total) : null}
              <div class="rounded-[10px] border border-line bg-panel p-3 my-3 text-[12px] space-y-2">
                <div class="flex items-center justify-between gap-3">
                  <span>{t('Downloading')} {p.tool}</span>
                  <span class="mono text-muted">{pct !== null ? Math.floor(pct) + '%' : ''}</span>
                </div>
                <div class="w-full h-[6px] bg-line rounded-full overflow-hidden">
                  <div class="h-full bg-accent rounded-full transition-[width] duration-300 {pct === null ? 'progress-live w-full' : ''}" style={pct !== null ? `width: ${pct}%` : ''}></div>
                </div>
                <div class="flex items-center justify-between gap-3 mono text-[11px] text-muted">
                  <span>{bytes(p.downloaded)}{p.total ? ' / ' + bytes(p.total) : ''}</span>
                  <span>{speed(p.speed)}{p.total && p.speed > 0 ? ' · ' + t('{time} left', { time: duration((p.total - p.downloaded) / p.speed) }) : ''}</span>
                </div>
              </div>
            {/if}
            <button class="btn btn-accent mt-2" disabled={checking} onclick={checkUpdates}><Icon name="refresh" size={13} />{checking ? '…' : t('Check for updates')}</button>
            {#if updates}
              <div class="mt-4 space-y-3 text-[12px]">
                <div class="rounded-[10px] border border-line bg-panel p-3 flex items-center justify-between">
                  <span>
                    Multron Download Manager {updates.app.current}
                    {#if updates.app.error}<span class="text-bad"> · {updates.app.error}</span>
                    {:else if updates.app.hasUpdate}<span class="text-warn"> → {updates.app.latest} {t('Update available')}</span>
                    {:else}<span class="text-ok"> · {t('Up to date')}{updates.app.latest ? '' : ' (no releases published yet)'}</span>{/if}
                  </span>
                  {#if app.updateReady}
                    <button class="btn btn-primary" onclick={() => app.installUpdate()}>{t('Restart and update')}</button>
                  {:else if updates.app.hasUpdate}
                    <button class="btn btn-primary" disabled={app.updateBusy} onclick={() => app.downloadUpdate()}>{app.updateBusy ? t('Downloading…') : t('Download and install')}</button>
                  {/if}
                </div>
                <div class="rounded-[10px] border border-line bg-panel p-3 flex items-center justify-between">
                  <span>
                    yt-dlp {updates.ytdlpInstalled ?? t('not installed')}
                    {#if updates.ytdlpError}<span class="text-bad"> · {updates.ytdlpError}</span>
                    {:else if updates.ytdlpLatest && updates.ytdlpLatest !== updates.ytdlpInstalled}<span class="text-warn"> → {updates.ytdlpLatest}</span>
                    {:else}<span class="text-ok"> · {t('Up to date')}</span>{/if}
                  </span>
                  {#if updates.ytdlpLatest && updates.ytdlpLatest !== updates.ytdlpInstalled}
                    <button class="btn" disabled={toolBusy !== null} onclick={() => runTool('ytdlp')}>{toolBusy ? '…' : t('Update')}</button>
                  {/if}
                </div>
              </div>
            {/if}
            <div class="flex gap-2 mt-5">
              <button class="btn" onclick={() => api.openUrl('https://github.com/Multron-Community/Multron-Download-Manager')}><Icon name="globe" size={13} />GitHub</button>
              <button class="btn" onclick={() => api.openUrl('https://discord.gg/xXmQw3MUAR')}><svg width="13" height="13" viewBox="-0.5 2 25 25" fill="#5865F2" aria-hidden="true"><path d="M17.5 4.96c-1.33-.6-2.76-1.03-4.26-1.25-.19.34-.4.75-.56 1.12-1.57-.23-3.14-.23-4.69 0-.17-.37-.38-.78-.57-1.12-1.5.22-2.93.65-4.26 1.25C.13 10.63-.4 16.2.22 21.68c1.78 1.3 3.51 2.08 5.23 2.6.43-.58.82-1.19 1.16-1.83-1.68-.62-2.31-1.65-2.31-1.65s.19.12.51.29c.14.07.28.14.41.21.32.14.65.26.98.37 2.1.72 4.31.96 6.55.96 2.23 0 4.45-.24 6.55-.96.33-.11.66-.23.98-.37.14-.07.27-.14.41-.21.32-.17.51-.29.51-.29s-.62 1.03-2.31 1.65c.34.64.73 1.25 1.16 1.83 1.72-.52 3.45-1.3 5.23-2.6.68-6.17-.43-11.64-2.92-16.72zM7.74 17.54c-1.14 0-2.07-1.04-2.07-2.31 0-1.27.92-2.31 2.07-2.31 1.16 0 2.08 1.05 2.07 2.31 0 1.27-.92 2.31-2.07 2.31zm8.52 0c-1.14 0-2.07-1.04-2.07-2.31 0-1.27.92-2.31 2.07-2.31 1.16 0 2.08 1.05 2.07 2.31 0 1.27-.92 2.31-2.07 2.31z" /></svg>Discord</button>
              <button class="btn" onclick={() => api.openUrl('https://www.patreon.com/cw/multron')}><svg width="12" height="12" viewBox="0 0 24 24" fill="#FF424D" aria-hidden="true"><path d="M15.386.524c-4.764 0-8.64 3.876-8.64 8.64 0 4.75 3.876 8.613 8.64 8.613 4.75 0 8.614-3.864 8.614-8.613C24 4.4 20.136.524 15.386.524M.003 23.537h4.22V.524H.003" /></svg>{t('Support Multron Download Manager on Patreon')}</button>
              <button class="btn" onclick={() => api.openUrl('https://github.com/Multron-Community/Multron-Download-Manager/releases')}>{t('Releases')}</button>
            </div>
          {/if}
        </div>
      </div>

      <div class="h-[52px] px-5 border-t border-line flex items-center justify-between flex-shrink-0 bg-panel">
        <span class="text-[11px] text-muted">{t('Changes apply as soon as you press Save.')}</span>
        <div class="flex gap-2">
          <button class="btn" disabled={!dirty} onclick={resetDraft}>{t('Cancel')}</button>
          <button class="btn btn-primary" disabled={!dirty} onclick={save}>{t('Save')}</button>
        </div>
      </div>
    </div>
  {/if}
</div>
