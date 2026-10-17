<script lang="ts">
  import { untrack } from 'svelte';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { app } from '../lib/store.svelte';
  import { api, errorText } from '../lib/api';
  import { t } from '../lib/i18n.svelte';
  import { bytes, duration } from '../lib/format';

  function focusOnMount(node: HTMLElement) {
    node.focus();
  }
  import type { AddRequest, MediaInfo, ProbeInfo, TorrentPreview } from '../types';
  import Icon from './Icon.svelte';

  type Mode = 'link' | 'media' | 'torrent';

  let mode = $state<Mode>('link');
  let url = $state('');
  let filename = $state('');
  let dir = $state('');
  let threads = $state(8);
  let speedLimit = $state(0);
  let sha256 = $state('');
  let shaSource = $state<string | null>(null);
  let startPaused = $state(false);
  let showAdvanced = $state(false);
  let busy = $state(false);

  let probe = $state<ProbeInfo | null>(null);
  let probing = $state(false);

  let media = $state<MediaInfo | null>(null);
  let mediaLoading = $state(false);
  let mediaError = $state<string | null>(null);
  let quality = $state('best');
  let playlist = $state(false);

  let preview = $state<TorrentPreview | null>(null);
  let torrentFile = $state<string | null>(null);
  let inspecting = $state(false);
  let torrentError = $state<string | null>(null);
  let picked = $state<Set<number>>(new Set());
  let trackersText = $state('');

  let extra: Partial<AddRequest> = {};
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
    quality = p?.quality ?? s?.mediaQuality ?? 'best';
    playlist = p?.playlist ?? false;
    probe = null;
    media = null;
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
      media = await api.mediaProbe(url.trim(), extra.cookies);
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
      picked = new Set(preview.files.map((_, i) => i));
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
      (mode === 'torrent' ? !!(torrentFile || url.trim()) && (!preview || picked.size > 0) : !!url.trim())
  );

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!canSubmit) return;
    busy = true;
    const base: AddRequest = {
      ...extra,
      url: url.trim(),
      dir,
      startPaused,
      speedLimit: Number(speedLimit) || 0,
      threads: Number(threads) || 8
    };
    let req: AddRequest;
    if (mode === 'link') {
      req = { ...base, kind: 'http', filename: filename.trim() || null, sha256: sha256.trim() || null };
    } else if (mode === 'media') {
      req = { ...base, kind: 'media', quality, playlist, pageTitle: media?.title ?? extra.pageTitle ?? null, thumbnail: media?.thumbnail ?? extra.thumbnail ?? null };
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
    const ok = await app.run(() => api.add(req));
    busy = false;
    if (ok) app.addOpen = false;
  }

  const qualities = [
    { id: 'best', label: 'Best' },
    { id: '2160', label: '2160p (4K)' },
    { id: '1440', label: '1440p' },
    { id: '1080', label: '1080p' },
    { id: '720', label: '720p' },
    { id: '480', label: '480p' },
    { id: '360', label: '360p' },
    { id: 'audio-mp3', label: 'Audio only (MP3)' },
    { id: 'audio-m4a', label: 'Audio only (M4A)' }
  ];
</script>

{#if app.addOpen}
  <div class="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4" role="presentation">
    <div class="w-full max-w-[620px] max-h-[92vh] bg-menu border border-line rounded-[14px] shadow-2xl flex flex-col">
      <div class="flex items-center justify-between px-5 pt-4 pb-3">
        <h2 class="text-[15px] font-semibold">{t('New Task')}</h2>
        <button type="button" class="icon-btn" aria-label={t('Close')} onclick={() => (app.addOpen = false)}><Icon name="x" size={14} /></button>
      </div>

      <div class="flex px-5 gap-1 border-b border-line">
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
          {#if probing}
            <p class="text-[11.5px] text-muted">{t('Checking link…')}</p>
          {:else if probe}
            <div class="rounded-[10px] border border-line bg-bg p-3 text-[12px] flex flex-wrap gap-x-4 gap-y-1">
              {#if probe.service}<span class="text-accent">{probe.service}</span>{/if}
              <span>{t('Size')}: {bytes(probe.total)}</span>
              <span class={probe.resumable ? 'text-ok' : 'text-warn'}>{probe.resumable ? `${t('Resumable')} · multi-connection` : t('Not resumable')}</span>
              {#if probe.contentType}<span class="text-muted">{probe.contentType}</span>{/if}
              {#if probe.error}<span class="text-warn w-full">{probe.error}</span>{/if}
            </div>
          {/if}
          <div class="space-y-1.5">
            <label for="fname" class="label">{t('File name')}</label>
            <input id="fname" class="field" bind:value={filename} placeholder="auto" />
          </div>
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
                {#if media.heights.length}<div class="text-dim">Available: {media.heights.slice(0, 6).map((h) => h + 'p').join(', ')}</div>{/if}
              </div>
            </div>
          {:else if mediaLoading}
            <p class="text-[11.5px] text-muted">{t('Asking yt-dlp… (the first run downloads yt-dlp from GitHub)')}</p>
          {/if}
          <div class="grid grid-cols-2 gap-3">
            <div class="space-y-1.5">
              <label for="q" class="label">{t('Quality')}</label>
              <select id="q" class="field" bind:value={quality}>
                {#each qualities as q}<option value={q.id}>{t(q.label)}</option>{/each}
              </select>
            </div>
            <label class="flex items-center gap-2 text-[12px] mt-6">
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
              <div class="max-h-[180px] overflow-y-auto">
                {#each preview.files as f, i}
                  <label class="flex items-center gap-2 px-3 py-1 text-[11.5px] hover:bg-hover">
                    <input type="checkbox" class="check" checked={picked.has(i)} onchange={() => { const s = new Set(picked); s.has(i) ? s.delete(i) : s.add(i); picked = s; }} />
                    <span class="truncate flex-1" title={f.name}>{f.name}</span>
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

        {#if mode === 'link' && (app.settings?.askSha256 || showAdvanced)}
          <div class="space-y-1.5">
            <label for="sha" class="label flex items-center gap-1.5"><Icon name="shield" size={12} />{t('Expected SHA-256')}{app.settings?.askSha256 ? '' : ' (optional)'}</label>
            <input id="sha" class="field mono {shaInvalid ? '!border-bad' : ''}" bind:value={sha256} placeholder={t('64 hex characters — the file is verified automatically when it finishes')} />
            {#if shaSource}<p class="text-[11px] text-ok truncate">{t('Auto-detected from')} {shaSource}</p>{/if}
            {#if shaInvalid}<p class="text-[11px] text-bad">{t('SHA-256 must be 64 hexadecimal characters.')}</p>{/if}
          </div>
        {/if}

        <button type="button" class="text-[11.5px] text-muted hover:text-white flex items-center gap-1 w-fit" onclick={() => (showAdvanced = !showAdvanced)}>
          <Icon name={showAdvanced ? 'chevronDown' : 'chevron'} size={11} />{t('Advanced')}
        </button>
        {#if showAdvanced}
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

        <div class="flex items-center justify-between pt-1">
          <label class="flex items-center gap-2 text-[12px] text-muted">
            <input type="checkbox" class="check" bind:checked={startPaused} />{t('Add paused')}
          </label>
          <div class="flex gap-2">
            <button type="button" class="btn" onclick={() => (app.addOpen = false)}>{t('Cancel')}</button>
            <button type="submit" class="btn btn-primary" disabled={!canSubmit}>{busy ? '…' : t('Start download')}</button>
          </div>
        </div>
      </form>
    </div>
  </div>
{/if}
