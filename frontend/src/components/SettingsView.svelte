<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { open as openDialog, ask } from '@tauri-apps/plugin-dialog';
  import { app } from '../lib/store.svelte';
  import { api, errorText } from '../lib/api';
  import { t, i18n, LANGUAGES } from '../lib/i18n.svelte';
  import { bytes, date, duration, percent, speed } from '../lib/format';
  import type { BridgeStatus, FilterStatus, Settings, ToolStatus, UpdateReport } from '../types';
  import Icon from './Icon.svelte';
  import { ACCENTS, MATERIALS } from '../lib/theme';

  interface Section {
    id: string;
    label: string;
    icon: string;
    parent?: string;
    keywords: string;
  }

  const sections: Section[] = [
    { id: 'appearance', label: 'Appearance', icon: 'palette', keywords: 'theme dark light system windows mode color accent transparency transparent mica acrylic blur vibrancy' },
    { id: 'general', label: 'Startup & Shutdown', icon: 'bolt', keywords: 'start minimized tray close language notifications resume' },
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
    { id: 'scheduler', label: 'Speed Scheduler', icon: 'clock', keywords: 'alternative speed limit turtle schedule time days night' },
    { id: 'automation', label: 'When Downloads Finish', icon: 'power', keywords: 'shutdown sleep quit command run program finish complete' },
    { id: 'components', label: 'Components', icon: 'puzzle', keywords: 'plugins components engine modules load startup' },
    { id: 'security', label: 'Security', icon: 'lock', keywords: 'tls certificate ssl pairing trusted extensions cookies' },
    { id: 'about', label: 'Updates & About', icon: 'info', keywords: 'version update check about license' }
  ];

  let draft = $state<Settings | null>(null);
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
      transparency: draft.transparency,
      backdropMaterial: draft.backdropMaterial,
      transparencyLevel: draft.transparencyLevel
    };
  });

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
      speedLimit: Number(draft.speedLimit) || 0,
      bridgePort: Number(draft.bridgePort),
      torrentPort: Number(draft.torrentPort),
      torrentPeerLimit: Number(draft.torrentPeerLimit),
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

  async function runTool(name: 'ytdlp' | 'ffmpeg') {
    toolBusy = name;
    const ok = await app.run(
      () => (name === 'ytdlp' ? api.updateYtdlp() : api.installFfmpeg()),
      name === 'ytdlp' ? 'yt-dlp is up to date' : 'FFmpeg installed'
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
  <label class="flex items-start gap-3 py-[7px] cursor-pointer">
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
  <h4 class="text-[12px] font-semibold text-white/90 mt-5 mb-1 pb-1 border-b border-line">{t(title)}</h4>
{/snippet}

<div class="flex-1 flex min-h-0 bg-bg">
  <aside class="w-[240px] border-r border-line bg-panel flex flex-col min-h-0">
    <div class="px-4 pt-4 pb-2 text-[14px] font-semibold flex items-center gap-2"><Icon name="settings" size={15} />{t('Options')}</div>
    <nav class="flex-1 overflow-y-auto px-2 space-y-[1px]">
      {#each visible as s}
        <button
          class="w-full h-[30px] rounded-[7px] flex items-center gap-2 text-[12.5px] cursor-pointer {s.parent ? 'pl-[30px]' : 'px-[10px]'} {app.settingsSection === s.id ? 'bg-accent-strong/25 text-white' : 'text-muted hover:bg-hover hover:text-white'}"
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
      <div class="h-[40px] px-5 flex items-center justify-between bg-accent-strong/90 flex-shrink-0">
        <span class="text-[14px] font-semibold">{t(current.label)}</span>
        {#if dirty}<span class="text-[11.5px] text-white/85">{t('Unsaved changes')}</span>{/if}
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
            </div>
            <p class="text-[11px] text-muted mt-1.5">{t('Used for buttons, progress bars and the tint of transparent backgrounds.')}</p>

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
          {:else if current.id === 'general'}
            {@render row('Language', 'By default Multron Download Manager uses your Windows, macOS or Linux language. Pick a language here to change it.')}
            <select class="field !w-[220px] mt-1" bind:value={draft.language}>
              <option value="auto">{t('System language')}</option>
              {#each LANGUAGES as l}<option value={l.code}>{l.name}</option>{/each}
            </select>
            {@render group('Startup')}
            {@render toggle(startupLabel, startupHint, draft.launchAtStartup, (v) => (draft!.launchAtStartup = v))}
            {@render toggle('Start minimized to the tray', '', draft.startMinimized, (v) => (draft!.startMinimized = v))}
            {@render toggle('Resume unfinished downloads on startup', '', draft.resumeOnStartup, (v) => (draft!.resumeOnStartup = v))}
            {@render group('Shutdown')}
            {@render toggle('Keep running in the background when the window is closed', 'Downloads, seeding and the browser bridge keep working. Quit from the tray icon.', draft.closeToTray, (v) => (draft!.closeToTray = v))}
            {@render toggle('Show notifications', 'When downloads finish, fail or a SHA-256 check completes.', draft.notifications, (v) => (draft!.notifications = v))}
          {:else if current.id === 'files'}
            {@render row('Default download folder')}
            <div class="flex gap-2 mt-1">
              <input class="field" bind:value={draft.downloadDir} />
              <button class="btn" onclick={browseDir}><Icon name="folder" size={13} />{t('Browse')}</button>
            </div>
            {@render toggle('Sort into category folders', 'Video, Music, Documents, Archives, Programs, Images and Other sub-folders.', draft.organizeByCategory, (v) => (draft!.organizeByCategory = v))}
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
            {@render row('User-Agent', 'Sent with direct downloads unless the browser extension provides its own.')}
            <input class="field mono mt-1" bind:value={draft.userAgent} />
          {:else if current.id === 'transfer'}
            {@render row('Global download speed limit (KB/s)', '0 = unlimited (use the full line). Applies to direct and media downloads; torrents have their own limits.')}
            <input type="number" min="0" class="field !w-[160px] mt-1" bind:value={draft.speedLimit} />
            <p class="text-[11px] text-muted mt-3">{t('Slower limits for certain hours are in Speed Scheduler.')}</p>
          {:else if current.id === 'scheduler'}
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
            {@render toggle('Confirm downloads sent from the browser', 'Opens the New Task dialog first so you can pick the folder, quality or SHA-256.', draft.confirmBrowserDownloads, (v) => (draft!.confirmBrowserDownloads = v))}
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
              {#each [['best', 'Best'], ['2160', '2160p'], ['1440', '1440p'], ['1080', '1080p'], ['720', '720p'], ['480', '480p'], ['audio-mp3', 'Audio only (MP3)'], ['audio-m4a', 'Audio only (M4A)']] as [v, l]}
                <option value={v}>{t(l)}</option>
              {/each}
            </select>
            {@render row('Video container')}
            <select class="field !w-[220px] mt-1" bind:value={draft.mediaContainer}>
              <option value="mp4">MP4</option><option value="mkv">MKV</option><option value="webm">WebM</option>
            </select>
            {@render row('Use cookies from browser', 'For private, age-restricted or login-only media (Instagram, private Reddit…). The extension already sends the page cookies; this is a fallback for links added by hand.')}
            <select class="field !w-[220px] mt-1" bind:value={draft.cookiesFromBrowser}>
              {#each browsers as b}<option value={b}>{b || 'None'}</option>{/each}
            </select>
            {@render toggle('Embed metadata', '', draft.embedMetadata, (v) => (draft!.embedMetadata = v))}
            {@render toggle('Embed thumbnail', '', draft.embedThumbnail, (v) => (draft!.embedThumbnail = v))}
          {:else if current.id === 'torrent'}
            {@render row('Incoming TCP / UDP listen port', 'Default 22155. Pick a port between 49160 and 65534 or keep the default; avoid 6881–6889 which many ISPs throttle. If the port is busy a random one is used.')}
            <input type="number" min="1" max="65535" class="field !w-[140px] mt-1" bind:value={draft.torrentPort} />
            {@render toggle('UPnP port forwarding', 'Opens the port on your router automatically.', draft.torrentUpnp, (v) => (draft!.torrentUpnp = v))}
            {@render toggle('uTP (µTP) transport', 'UDP-based peer connections in addition to TCP.', draft.torrentUtp, (v) => (draft!.torrentUtp = v))}
            {@render toggle('DHT (trackerless peers)', '', draft.torrentDht, (v) => (draft!.torrentDht = v))}
            {@render toggle('Local Service Discovery', 'Finds peers on your local network.', draft.torrentLsd, (v) => (draft!.torrentLsd = v))}
            {@render row('Maximum peers per torrent')}
            <input type="number" min="10" max="2000" class="field !w-[120px] mt-1" bind:value={draft.torrentPeerLimit} />
            <p class="text-[11px] text-muted mt-3">{t('Changing these restarts the torrent engine; running torrents continue where they left off.')}</p>
          {:else if current.id === 'queue'}
            {@render row('Maximum active downloading torrents', 'The rest wait in the queue and start in order. 0 = no limit.')}
            <input type="number" min="0" max="200" class="field !w-[120px] mt-1" bind:value={draft.torrentMaxActiveDownloads} />
            {@render row('Maximum active seeding torrents', '0 = no limit.')}
            <input type="number" min="0" max="500" class="field !w-[120px] mt-1" bind:value={draft.torrentMaxActiveSeeds} />
            <p class="text-[11px] text-muted mt-3">{t('Reorder queued torrents with the arrows on each download. Paused torrents do not count.')}</p>
          {:else if current.id === 'watch'}
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
            {@render row('Auto reload period [days]')}
            <input type="number" min="1" max="365" class="field !w-[100px] mt-1" bind:value={draft.ipFilterReloadDays} />
            <p class="text-[11px] text-muted mt-2 leading-relaxed">
              {t('Supports DAT (eMule), P2P (PeerGuardian) and CIDR lists. Files can be local or URLs, plain, gzipped or zipped. URLs are downloaded again after the reload period.')}
            </p>
            {@render group('Manual ranges')}
            <textarea class="field" rows="5" placeholder={'Bad peers:10.0.0.0-10.0.0.255\n192.168.5.0/24\n203.0.113.7'} bind:value={draft.ipFilterManual}></textarea>
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
                <tr><td><input type="checkbox" class="check" bind:checked={draft.bridgeEnabled} /></td><td>{t('Built in')}</td><td>{t('Browser extension bridge')}</td><td>{app.version}</td><td>{bridge?.running ? '● Listening' : 'Off'}</td></tr>
                <tr><td><input type="checkbox" class="check" bind:checked={draft.torrentScrape} /></td><td>{t('Built in')}</td><td>{t('Tracker scraper')}</td><td>{app.version}</td><td>{draft.torrentScrape ? 'On' : 'Off'}</td></tr>
                <tr><td><input type="checkbox" class="check" bind:checked={draft.torrentUpnp} /></td><td>{t('Built in')}</td><td>{t('Universal Plug and Play (UPnP)')}</td><td>—</td><td>{draft.torrentUpnp ? 'On' : 'Off'}</td></tr>
                <tr><td><input type="checkbox" class="check" bind:checked={draft.torrentUtp} /></td><td>{t('Built in')}</td><td>{t('uTP transport')}</td><td>—</td><td>{draft.torrentUtp ? 'On' : 'Off'}</td></tr>
                <tr><td><input type="checkbox" class="check" bind:checked={draft.torrentDht} /></td><td>{t('Built in')}</td><td>{t('Mainline DHT')}</td><td>—</td><td>{app.stats.torrent?.dhtNodes != null ? `${app.stats.torrent.dhtNodes} nodes` : draft.torrentDht ? 'On' : 'Off'}</td></tr>
                <tr><td><input type="checkbox" class="check" bind:checked={draft.torrentLsd} /></td><td>{t('Built in')}</td><td>{t('Local Service Discovery')}</td><td>—</td><td>{draft.torrentLsd ? 'On' : 'Off'}</td></tr>
                <tr><td><input type="checkbox" class="check" bind:checked={draft.ipFilterEnabled} /></td><td>{t('Built in')}</td><td>{t('IP filter')}</td><td>—</td><td>{filterStatus?.rules ? `${filterStatus.rules} ranges` : 'Empty'}</td></tr>
              </tbody>
            </table>
          {:else if current.id === 'security'}
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
            {#if app.platform === 'windows'}
              {@render group('Antivirus scan')}
              {@render toggle('Scan finished downloads with the antivirus', 'Uses Microsoft Defender, or the antivirus installed on this PC through Windows AMSI (files up to 50 MB). A found threat is shown on the download and in a notification.', draft.scanDownloads, (v) => (draft!.scanDownloads = v))}
            {/if}
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
