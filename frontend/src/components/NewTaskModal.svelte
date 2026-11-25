<script lang="ts">
  import { CONTAINERS, QUALITIES } from '../lib/qualities';
  import { linkBatch, linksFromText } from '../lib/links';
  import { grabber } from '../lib/grabber.svelte';
  import { untrack } from 'svelte';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { app, PROGRAM_FILE } from '../lib/store.svelte';
  import { api, errorText } from '../lib/api';
  import { t } from '../lib/i18n.svelte';
  import { bytes, duration } from '../lib/format';

  function focusOnMount(node: HTMLElement) {
    node.focus();
  }
  import type { AddRequest, ArchiveEntry, ArchiveListing, MediaInfo, ProbeInfo, TorrentPreview } from '../types';
  import ArchiveBrowser from './ArchiveBrowser.svelte';
  import Icon from './Icon.svelte';

  type Mode = 'link' | 'media' | 'torrent';

  let { windowed = false, onAdded, onClose }: { windowed?: boolean; onAdded?: (id: string) => void; onClose?: () => void } = $props();

  function close() {
    app.addOpen = false;
    onClose?.();
  }

  let mode = $state<Mode>('link');
  let url = $state('');
  let filename = $state('');
  let dir = $state('');
  let threads = $state(8);
  let speedLimit = $state(0);
  let sha256 = $state('');
  let shaSource = $state<string | null>(null);
  let startPaused = $state(false);
  let mirrorsText = $state('');
  let tagsText = $state('');
  let useLogin = $state(false);
  let loginUser = $state('');
  let loginPass = $state('');
  let rememberLogin = $state(true);
  let laterClicked = false;
  let savedLogin = $derived.by(() => {
    try {
      const h = new URL(url.trim()).hostname.toLowerCase().replace(/^www\./, '');
      return (app.settings?.siteLogins ?? []).find((l) => l.enabled && (h === l.host || (l.subdomains && h.endsWith('.' + l.host)))) ?? null;
    } catch {
      return null;
    }
  });
  let showAdvanced = $state(false);
  let busy = $state(false);

  let probe = $state<ProbeInfo | null>(null);
  let probing = $state(false);
  let browsing = $state(false);
  let archiveListing = $state<ArchiveListing | null>(null);
  let archivePicked = $state<ArchiveEntry[]>([]);
  let archiveFolders = $state(false);
  const ARCHIVE_NAME = /\.(zip|zipx|jar|war|ear|apk|apks|xapk|aab|ipa|xpi|crx|whl|nupkg|vsix|appx|msix|docx|xlsx|pptx|odt|ods|odp|epub|cbz|iso|img|udf|tar|7z|cb7|rar|cbr)($|[?#])/i;
  const ARCHIVE_TYPE = /zip|iso9660|x-iso|x-tar|x-7z|x-rar|vnd\.rar|java-archive|android\.package|epub|openxmlformats|opendocument/i;
  let canBrowse = $derived(
    !!probe && probe.resumable && !!probe.total && !probe.service && (ARCHIVE_NAME.test(probe.filename ?? '') || ARCHIVE_NAME.test(url.trim()) || ARCHIVE_TYPE.test(probe.contentType ?? ''))
  );

  let media = $state<MediaInfo | null>(null);
  let mediaLoading = $state(false);
  let mediaError = $state<string | null>(null);
  let quality = $state('best');
  let several = $state(false);
  let severalText = $state('');
  const severalLinks = $derived(linksFromText(severalText));

  function reviewSeveral() {
    grabber.add(linkBatch(severalLinks, t('Several links')).items);
    app.view = 'grabber';
    severalText = '';
    several = false;
    app.addOpen = false;
  }
  let container = $state('mp4-h264');
  let playlist = $state(false);

  let preview = $state<TorrentPreview | null>(null);
  let torrentFile = $state<string | null>(null);
  let inspecting = $state(false);
  let torrentError = $state<string | null>(null);
  let picked = $state<Set<number>>(new Set());
  let trackersText = $state('');

  let extra = $state<Partial<AddRequest>>({});
  let probeTimer: ReturnType<typeof setTimeout> | undefined;
  let lastProbed = '';

  function detectMode(u: string, kind?: string | null): Mode {
    if (kind === 'torrent' || kind === 'media') return kind;
    if (kind === 'http') return 'link';
    const l = u.trim().toLowerCase();
    if (l.startsWith('magnet:') || l.endsWith('.torrent')) return 'torrent';
    if (/^(https?:\/\/)?([a-z0-9-]+\.)*(youtube\.com|youtu\.be|reddit\.com|redd\.it|instagram\.com|tiktok\.com|twitter\.com|x\.com|facebook\.com|fb\.watch|vimeo\.com|dailymotion\.com|twitch\.tv|soundcloud\.com|bilibili\.com|streamable\.com|threads\.net|kick\.com|pinterest\.com)\//.test(l)) return 'media';
    return 'link';
  }

  $effect(() => {
    if (app.addOpen) untrack(reset);
  });

  function reset() {
    const p = app.addPrefill;
    const s = app.settings;
    extra = p ?? {};
    url = p?.url ?? '';
    filename = p?.filename ?? '';
    dir = p?.dir ?? s?.downloadDir ?? '';
    threads = p?.threads ?? s?.threads ?? 8;
    speedLimit = 0;
    sha256 = p?.sha256 ?? '';
    shaSource = null;
    startPaused = false;
    mirrorsText = '';
    tagsText = '';
    useLogin = false;
    loginUser = '';
    loginPass = '';
    rememberLogin = true;
    quality = p?.quality ?? s?.mediaQuality ?? 'best';
    container = s?.mediaContainer ?? 'mp4-h264';
    playlist = p?.playlist ?? false;
    probe = null;
    media = null;
    browsing = false;
    archiveListing = null;
    archivePicked = [];
    archiveFolders = false;
    mediaError = null;
    preview = null;
    torrentError = null;
    torrentFile = app.addTorrentFile;
    picked = new Set();
    trackersText = (p?.trackers ?? []).join('\n');
    lastProbed = '';
    showAdvanced = !!s?.askSha256;
    mode = torrentFile ? 'torrent' : detectMode(url, p?.kind);
    if (torrentFile) inspectTorrent();
    else if (url && mode === 'media') fetchMedia();
    else if (url && mode === 'link') runProbe();
  }

  function onUrlInput() {
    const m = detectMode(url);
    if (m !== mode && !torrentFile) mode = m;
    browsing = false;
    clearTimeout(probeTimer);
    if (mode === 'link') probeTimer = setTimeout(runProbe, 650);
  }

  async function runProbe() {
    const u = url.trim();
    if (!/^https?:\/\//i.test(u) || u === lastProbed) return;
    lastProbed = u;
    probing = true;
    try {
      const p = await api.probeUrl(u, extra.referrer, extra.cookies);
      if (url.trim() !== u) return;
      probe = p;
      if (p.filename && !filename) filename = p.filename;
      if (p.sha256 && !sha256) {
        sha256 = p.sha256;
        shaSource = p.sha256Source;
      }
      if (p.kind === 'media' && mode === 'link') {
        mode = 'media';
        fetchMedia();
      }
    } catch (e) {
      probe = null;
    } finally {
      probing = false;
    }
  }

  async function fetchMedia() {
    if (!url.trim()) return;
    mediaLoading = true;
    mediaError = null;
    try {
      media = extra.stream
        ? await api.mediaProbe(url.trim(), extra.cookies, extra.referrer, extra.userAgent)
        : await api.mediaProbe(url.trim(), extra.cookies);
      if (media.isPlaylist) playlist = true;
    } catch (e) {
      mediaError = errorText(e);
      media = null;
    } finally {
      mediaLoading = false;
    }
  }

  async function inspectTorrent() {
    if (!torrentFile && !url.trim()) return;
    inspecting = true;
    torrentError = null;
    preview = null;
    try {
      preview = await api.torrentInspect(url.trim(), torrentFile);
      const skip = app.settings?.skipTorrentPrograms !== false;
      picked = new Set(preview.files.map((_, i) => i).filter((i) => !(skip && PROGRAM_FILE.test(preview!.files[i].name)) && !isJunk(preview!.files[i].name)));
    } catch (e) {
      torrentError = errorText(e);
    } finally {
      inspecting = false;
    }
  }

  async function chooseDir() {
    const res = await openDialog({ directory: true, defaultPath: dir || undefined });
    if (typeof res === 'string') dir = res;
  }

  async function chooseTorrent() {
    const res = await openDialog({ filters: [{ name: 'Torrent', extensions: ['torrent'] }] });
    if (typeof res === 'string') {
      torrentFile = res;
      url = '';
      inspectTorrent();
    }
  }

  async function addPublicTrackers() {
    try {
      const list = await api.publicTrackers();
      const current = new Set(trackersText.split('\n').map((s) => s.trim()).filter(Boolean));
      list.forEach((x) => current.add(x));
      trackersText = [...current].join('\n');
    } catch (e) {
      app.toast('error', errorText(e));
    }
  }

  let shaInvalid = $derived(sha256.trim() !== '' && !/^[0-9a-fA-F]{64}$/.test(sha256.trim().split(/\s+/)[0]));
  let canSubmit = $derived(
    !busy &&
      !shaInvalid &&
      (mode === 'torrent' ? !!(torrentFile || url.trim()) && (!preview || picked.size > 0) : !!url.trim()) &&
      !(mode === 'link' && browsing && (!archiveListing || !archivePicked.length))
  );

  async function addFromArchive(base: AddRequest, later: boolean) {
    const listing = archiveListing!;
    let first: string | null = null;
    let added = 0;
    for (const entry of archivePicked) {
      const id = await app.addChecked({
        ...base,
        kind: 'http',
        filename: null,
        sha256: null,
        mirrors: [],
        later,
        total: entry.size,
        archive: { format: listing.format, archiveSize: listing.size, entry },
        archiveFolders
      });
      if (id) {
        first ??= id;
        added++;
      }
    }
    return added ? first : null;
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!canSubmit) return;
    busy = true;
    const later = laterClicked;
    laterClicked = false;
    const base: AddRequest = {
      ...extra,
      url: url.trim(),
      dir,
      startPaused,
      later,
      mirrors: mode === 'link' ? mirrorsText.split('\n').map((x) => x.trim()).filter(Boolean) : [],
      tags: tagsText.split(',').map((x) => x.trim()).filter(Boolean),
      loginUser: useLogin && mode !== 'torrent' ? loginUser.trim() || null : null,
      loginPassword: useLogin && mode !== 'torrent' ? loginPass : null,
      rememberLogin: useLogin && rememberLogin,
      speedLimit: Number(speedLimit) || 0,
      threads: Number(threads) || 8
    };
    if (mode === 'link' && browsing && archiveListing) {
      const first = await addFromArchive(base, later);
      busy = false;
      if (first !== null) {
        app.addOpen = false;
        if (first) onAdded?.(first);
        else onClose?.();
      }
      return;
    }
    let req: AddRequest;
    if (mode === 'link') {
      req = { ...base, kind: 'http', filename: filename.trim() || null, sha256: sha256.trim() || null };
    } else if (mode === 'media') {
      req = { ...base, kind: 'media', quality, container: quality.startsWith('audio') ? null : container, playlist, pageTitle: media?.title ?? extra.pageTitle ?? null, thumbnail: media?.thumbnail ?? extra.thumbnail ?? null };
    } else {
      req = {
        ...base,
        kind: 'torrent',
        torrentFile,
        infoHash: preview?.infoHash ?? null,
        pageTitle: preview?.name ?? null,
        total: preview ? preview.files.filter((_, i) => picked.has(i)).reduce((a, f) => a + f.length, 0) : null,
        onlyFiles: preview && picked.size < preview.files.length ? [...picked] : null,
        trackers: trackersText.split('\n').map((s) => s.trim()).filter(Boolean)
      };
    }
    const id = await app.addChecked(req);
    busy = false;
    if (id !== null) {
      app.addOpen = false;
      if (id) onAdded?.(id);
      else onClose?.();
    }
  }

  let topHeight = $derived(media?.heights.length ? Math.max(...media.heights) : 0);
  let audioOnly = $derived(app.settings?.siteOptions !== false && media?.audioOnly === true);
  let qualities = $derived(
    audioOnly
      ? QUALITIES.filter((q) => q.id.startsWith('audio'))
      : app.settings?.mediaAvailableQualities !== false && topHeight
        ? QUALITIES.filter((q) => !/^\d+$/.test(q.id) || Number(q.id) <= topHeight)
        : QUALITIES
  );
  let junkPatterns = $derived(
    app.settings?.torrentSkipEnabled
      ? (app.settings.torrentSkipPatterns ?? '').split(/[,;\n]+/).map((x) => x.trim().toLowerCase()).filter(Boolean)
      : []
  );
  function isJunk(name: string) {
    const n = name.toLowerCase();
    const base = n.split(/[\\/]/).pop() ?? n;
    return junkPatterns.some((p) => (p.startsWith('.') ? base.endsWith(p) : base.includes(p)));
  }
  let junkInTorrent = $derived(preview && junkPatterns.length ? preview.files.filter((f) => isJunk(f.name)).length : 0);

  $effect(() => {
    const s = app.settings;
    if (mode === 'torrent' && s?.torrentDir && untrack(() => dir.trim().replace(/[\\/]+$/, '').toLowerCase()) === (s.downloadDir ?? '').trim().replace(/[\\/]+$/, '').toLowerCase()) {
      dir = s.torrentDir;
    }
  });
  let programsInTorrent = $derived(preview ? preview.files.filter((f) => PROGRAM_FILE.test(f.name)).length : 0);

  $effect(() => {
    if (audioOnly && !quality.startsWith('audio')) quality = 'audio-mp3';
    else if (/^\d+$/.test(quality) && !qualities.some((q) => q.id === quality)) quality = 'best';
  });
</script>

<svelte:window onkeydown={(e) => !windowed && e.key === 'Escape' && close()} />

{#if app.addOpen}
  <div class={windowed ? 'fixed inset-0 flex bg-bg' : 'fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4'} role="presentation">
    <div class={windowed ? 'w-full h-full flex flex-col bg-bg' : 'w-full max-w-[620px] max-h-[92vh] bg-menu border border-line rounded-[14px] shadow-2xl flex flex-col'}>
      <div class="flex items-center justify-between px-5 pt-4 pb-3">
        <h2 class="title-glow text-[15px] font-semibold flex items-center gap-2.5"><span class="settings-title-icon w-[30px] h-[30px] rounded-[9px] flex items-center justify-center flex-shrink-0"><Icon name={mode === 'media' ? 'film' : mode === 'torrent' ? 'magnet' : 'down'} size={15} /></span>{windowed ? t('Download file') : t('New Task')}</h2>
        {#if !windowed}<button type="button" class="icon-btn" aria-label={t('Close')} onclick={close}><Icon name="x" size={14} /></button>{/if}
      </div>

      <div class="pill-tabs flex px-5 pb-2 gap-1 border-b border-line">
        {#each [['link', 'Link', 'link'], ['media', 'Media', 'film'], ['torrent', 'Torrent', 'magnet']] as [id, label, icon]}
          <button type="button" class="tab flex items-center gap-1.5 {mode === id ? 'tab-active' : ''}" onclick={() => (mode = id as Mode)}>
            <Icon name={icon} size={13} />{t(label)}
          </button>
        {/each}
      </div>

      <form onsubmit={submit} class="flex flex-col gap-4 p-5 overflow-y-auto">
        {#if mode !== 'torrent' || !torrentFile}
          <div class="space-y-1.5">
            <label for="url" class="label">{mode === 'torrent' ? t('Magnet link or .torrent URL') : t('Download link')}</label>
            <div class="flex gap-2">
              <input id="url" class="field" use:focusOnMount placeholder={mode === 'torrent' ? 'magnet:?xt=urn:btih:…' : mode === 'media' ? 'https://www.youtube.com/watch?v=…' : 'https://example.com/file.zip'} bind:value={url} oninput={onUrlInput} />
              {#if mode === 'media'}
                <button type="button" class="btn" onclick={fetchMedia} disabled={mediaLoading}>{mediaLoading ? '…' : t('Fetch info')}</button>
              {:else if mode === 'torrent'}
                <button type="button" class="btn" onclick={inspectTorrent} disabled={inspecting || !url.trim()}>{inspecting ? '…' : t('Fetch info')}</button>
              {/if}
            </div>
          </div>
        {/if}

        {#if mode === 'link'}
          <div class="space-y-1.5 {windowed ? 'hidden' : ''}">
            <button type="button" class="text-[11.5px] text-accent hover:underline w-fit" onclick={() => (several = !several)}>{several ? t('Hide') : t('Add several links at once…')}</button>
            {#if several}
              <textarea class="field !h-[110px] py-2 mono text-[11.5px] resize-y" placeholder={t('One link per line. A number range like file[01-20].zip adds every number.')} bind:value={severalText}></textarea>
              <div class="flex items-center gap-2">
                <button type="button" class="btn btn-accent" disabled={!severalLinks.length} onclick={reviewSeveral}>{t('Review {count} links', { count: severalLinks.length })}</button>
              </div>
            {/if}
          </div>
          {#if probing}
            <p class="text-[11.5px] text-muted">{t('Checking link…')}</p>
          {:else if probe}
            <div class="rounded-[10px] border border-line bg-bg p-3 text-[12px] flex flex-wrap gap-x-4 gap-y-1">
              {#if probe.service}<span class="text-accent">{probe.service}</span>{/if}
              <span>{t('Size')}: {bytes(probe.total)}</span>
              <span class={probe.resumable ? 'text-ok' : 'text-warn'}>{probe.resumable ? `${t('Resumable')} · multi-connection` : t('Not resumable')}</span>
              {#if probe.contentType}<span class="text-muted">{probe.contentType}</span>{/if}
              {#if probe.error}<span class="text-warn w-full">{probe.error}</span>{/if}
              {#if canBrowse && !browsing}
                <button type="button" class="text-accent hover:underline flex items-center gap-1 w-full mt-0.5" onclick={() => (browsing = true)}>
                  <Icon name="archive" size={12} />{t('Browse archive contents')}
                </button>
              {/if}
            </div>
          {/if}
          {#if browsing}
            <ArchiveBrowser url={url.trim()} referrer={extra.referrer ?? null} cookies={extra.cookies ?? []} bind:listing={archiveListing} bind:selected={archivePicked} bind:folders={archiveFolders} onClose={() => (browsing = false)} />
          {:else}
            <div class="space-y-1.5">
              <label for="fname" class="label">{t('File name')}</label>
              <input id="fname" class="field" bind:value={filename} placeholder="auto" />
            </div>
          {/if}
        {:else if mode === 'media'}
          {#if mediaError}
            <p class="text-[12px] text-bad whitespace-pre-wrap">{mediaError}</p>
          {/if}
          {#if media}
            <div class="flex gap-3 rounded-[10px] border border-line bg-bg p-3">
              {#if media.thumbnail}
                <img src={media.thumbnail} alt="" class="w-[120px] h-[68px] rounded-[6px] object-cover flex-shrink-0" />
              {/if}
              <div class="min-w-0 text-[12px] space-y-1">
                <div class="font-medium truncate">{media.title}</div>
                <div class="text-muted truncate">{[media.site, media.uploader, media.duration ? duration(media.duration) : null, media.isPlaylist ? `${media.entries ?? '?'} items` : null].filter(Boolean).join(' · ')}</div>
                {#if media.heights.length}<div class="text-dim">{t('Available qualities: {list}', { list: media.heights.map((h) => h + 'p').join(', ') })}</div>{/if}
              </div>
            </div>
          {:else if mediaLoading}
            <p class="text-[11.5px] text-muted">{t('Asking yt-dlp… (the first run downloads yt-dlp from GitHub)')}</p>
          {/if}
          <div class="grid grid-cols-2 gap-3">
            <div class="space-y-1.5">
              <label for="q" class="label">{t('Quality')}</label>
              <select id="q" class="field" bind:value={quality}>
                {#each qualities as q}{@const s = media?.sizes?.[q.id]}<option value={q.id}>{t(q.label)}{q.id === 'best' && topHeight ? ` (${topHeight}p)` : ''}{s ? ` · ${s.exact ? '' : '~'}${bytes(s.size)}` : ''}</option>{/each}
              </select>
            </div>
            <div class="space-y-1.5">
              <label for="container" class="label">{t('Video format')}</label>
              <select id="container" class="field" bind:value={container} disabled={quality.startsWith('audio')}>
                {#each CONTAINERS as c}<option value={c.id}>{c.label}</option>{/each}
              </select>
            </div>
            <label class="flex items-center gap-2 text-[12px] col-span-2">
              <input type="checkbox" class="check" bind:checked={playlist} />{t('Download whole playlist')}
            </label>
          </div>
        {:else}
          <div class="flex items-center gap-2">
            <button type="button" class="btn" onclick={chooseTorrent}><Icon name="file" size={13} />{t('Open .torrent file')}</button>
            {#if torrentFile}
              <span class="text-[11.5px] text-muted truncate" title={torrentFile}>{torrentFile}</span>
              <button type="button" class="icon-btn" aria-label={t('Clear')} onclick={() => { torrentFile = null; preview = null; }}><Icon name="x" size={12} /></button>
            {/if}
          </div>
          {#if inspecting}
            <p class="text-[11.5px] text-muted">{t('Fetching metadata from peers… (magnet links can take a moment)')}</p>
          {/if}
          {#if torrentError}<p class="text-[12px] text-bad">{torrentError}</p>{/if}
          {#if preview}
            <div class="rounded-[10px] border border-line bg-bg">
              <div class="flex items-center justify-between px-3 py-2 border-b border-line text-[12px]">
                <span class="font-medium truncate">{preview.name}</span>
                <span class="text-muted flex-shrink-0">{bytes(preview.files.filter((_, i) => picked.has(i)).reduce((a, f) => a + f.length, 0))} / {bytes(preview.total)}{preview.private ? ' · private' : ''}</span>
              </div>
              <div class="flex gap-2 px-3 py-1.5 border-b border-line">
                <button type="button" class="text-[11px] text-accent hover:underline" onclick={() => (picked = new Set(preview!.files.map((_, i) => i)))}>{t('Select all')}</button>
                <button type="button" class="text-[11px] text-accent hover:underline" onclick={() => (picked = new Set())}>{t('Select none')}</button>
              </div>
              {#if junkInTorrent}
                <div class="px-3 py-1.5 border-b border-line text-[11px] text-muted flex items-start gap-1.5">
                  <Icon name="filter" size={12} class="mt-[1px] flex-shrink-0" />
                  <span>{t('{count} extra files (samples, .nfo, .txt, …) were left unticked. Settings → Torrent.', { count: junkInTorrent })}</span>
                </div>
              {/if}
              {#if programsInTorrent}
                <div class="px-3 py-1.5 border-b border-line text-[11px] text-warn flex items-start gap-1.5">
                  <Icon name="alert" size={12} class="mt-[1px] flex-shrink-0" />
                  <span>{app.settings?.skipTorrentPrograms !== false ? t('This torrent contains {count} program files. They were left unticked, because fake videos and games often hide malware in them.', { count: programsInTorrent }) : t('This torrent contains {count} program files. Fake videos and games often hide malware in them.', { count: programsInTorrent })}</span>
                </div>
              {/if}
              <div class="max-h-[180px] overflow-y-auto">
                {#each preview.files as f, i}
                  <label class="flex items-center gap-2 px-3 py-1 text-[11.5px] hover:bg-hover">
                    <input type="checkbox" class="check" checked={picked.has(i)} onchange={() => { const s = new Set(picked); s.has(i) ? s.delete(i) : s.add(i); picked = s; }} />
                    <span class="truncate flex-1 {PROGRAM_FILE.test(f.name) ? 'text-warn' : ''}" title={f.name}>{f.name}</span>
                    <span class="text-muted flex-shrink-0">{bytes(f.length)}</span>
                  </label>
                {/each}
              </div>
            </div>
          {/if}
          <div class="space-y-1.5">
            <div class="flex items-center justify-between">
              <label for="tr" class="label">{t('Extra trackers (one per line)')}</label>
              <button type="button" class="text-[11px] text-accent hover:underline" onclick={addPublicTrackers}>+ {t('public trackers')}</button>
            </div>
            <textarea id="tr" class="field" rows="3" bind:value={trackersText} placeholder="udp://tracker.example.org:1337/announce"></textarea>
          </div>
        {/if}

        <div class="space-y-1.5">
          <label for="dir" class="label">{t('Save to')}</label>
          <div class="flex gap-2">
            <input id="dir" class="field" bind:value={dir} />
            <button type="button" class="btn" onclick={chooseDir}><Icon name="folder" size={13} />{t('Browse')}</button>
          </div>
        </div>

        {#if mode === 'link' && !browsing && (app.settings?.askSha256 || showAdvanced)}
          <div class="space-y-1.5">
            <label for="sha" class="label flex items-center gap-1.5"><Icon name="shield" size={12} />{t('Expected SHA-256')}{app.settings?.askSha256 ? '' : ' (optional)'}</label>
            <input id="sha" class="field mono {shaInvalid ? '!border-bad' : ''}" bind:value={sha256} placeholder={t('64 hex characters — the file is verified automatically when it finishes')} />
            {#if shaSource}<p class="text-[11px] text-ok truncate">{t('Auto-detected from')} {shaSource}</p>{/if}
            {#if shaInvalid}<p class="text-[11px] text-bad">{t('SHA-256 must be 64 hexadecimal characters.')}</p>{/if}
          </div>
        {/if}

        {#if mode === 'link'}
          <div class="rounded-[10px] border border-line p-3 space-y-2">
            <label class="flex items-center gap-2 text-[12.5px] cursor-pointer"><input type="checkbox" class="check" bind:checked={useLogin} /><Icon name="lock" size={12} />{t('Use authorization')}</label>
            {#if savedLogin && !useLogin}
              <p class="text-[11px] text-ok">{t('A saved login for {site} ({user}) is used automatically.', { site: savedLogin.host, user: savedLogin.user || '—' })}</p>
            {/if}
            {#if useLogin}
              <div class="grid grid-cols-2 gap-2">
                <input class="field" placeholder={t('User name')} autocomplete="username" bind:value={loginUser} />
                <input class="field" type="password" placeholder={t('Password')} autocomplete="current-password" bind:value={loginPass} />
              </div>
              <label class="flex items-center gap-2 text-[11.5px] text-muted cursor-pointer"><input type="checkbox" class="check" bind:checked={rememberLogin} />{t('Remember for this site, so its other downloads log in by themselves')}</label>
            {/if}
          </div>
        {/if}

        <button type="button" class="text-[11.5px] text-muted hover:text-white flex items-center gap-1 w-fit" onclick={() => (showAdvanced = !showAdvanced)}>
          <Icon name={showAdvanced ? 'chevronDown' : 'chevron'} size={11} />{t('Advanced')}
        </button>
        {#if showAdvanced}
          <div class="space-y-1.5">
            <label for="tags" class="label">{t('Tags')}</label>
            <input id="tags" class="field" placeholder={t('Comma separated, for example work, photos')} bind:value={tagsText} />
          </div>
          {#if mode === 'link'}
            <div class="space-y-1.5">
              <label for="mirrors" class="label">{t('Mirror links')}</label>
              <textarea id="mirrors" class="field !h-[60px] py-1.5 mono text-[11.5px] resize-y" placeholder="https://mirror.example.com/file.zip" bind:value={mirrorsText}></textarea>
              <p class="text-[11px] text-muted">{t('One per line. If the download fails, it continues from the next mirror by itself.')}</p>
            </div>
          {/if}
          <div class="grid grid-cols-2 gap-3">
            {#if mode !== 'torrent'}
              <div class="space-y-1.5">
                <label for="th" class="label">{t('Connections')} (1–64)</label>
                <input id="th" type="number" min="1" max="64" class="field" bind:value={threads} />
              </div>
            {/if}
            <div class="space-y-1.5">
              <label for="sl" class="label">{t('Speed limit')} — {t('KB/s (0 = unlimited)')}</label>
              <input id="sl" type="number" min="0" class="field" bind:value={speedLimit} disabled={mode === 'torrent'} />
            </div>
          </div>
        {/if}

        <div class="modal-foot flex items-center justify-between -mx-5 -mb-5 px-5 py-3 mt-1">
          <label class="flex items-center gap-2 text-[12px] text-muted">
            <input type="checkbox" class="check" bind:checked={startPaused} />{t('Add paused')}
          </label>
          <div class="flex gap-2">
            <button type="button" class="btn" onclick={close}>{t('Cancel')}</button>
            <button type="submit" class="btn" disabled={!canSubmit} title={t('Adds it to the queue without starting; the scheduler starts it (Settings → Scheduler).')} onclick={() => (laterClicked = true)}><Icon name="clock" size={12} />{t('Download later')}</button>
            <button type="submit" class="btn btn-primary" disabled={!canSubmit} onclick={() => (laterClicked = false)}>{busy ? '…' : t('Start download')}</button>
          </div>
        </div>
      </form>
    </div>
  </div>
{/if}
