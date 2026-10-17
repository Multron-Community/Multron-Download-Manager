<script lang="ts">
  import { onMount } from 'svelte';
  import { save as saveDialog } from '@tauri-apps/plugin-dialog';
  import { app } from '../lib/store.svelte';
  import { api, errorText } from '../lib/api';
  import { t, i18n } from '../lib/i18n.svelte';
  import { bytes, date, decodeBits, duration, percent, speed } from '../lib/format';
  import type { Download, TorrentDetails } from '../types';
  import Icon from './Icon.svelte';
  import SpeedGraph from './SpeedGraph.svelte';

  interface Props {
    item: Download;
    onClose: () => void;
  }
  let { item, onClose }: Props = $props();

  const tabs = ['speed', 'files', 'options', 'sources', 'peers', 'pieces', 'info', 'privacy', 'log'];
  const labels: Record<string, string> = {
    speed: 'Speed',
    files: 'Files',
    options: 'Options',
    sources: 'Sources',
    peers: 'Peers',
    pieces: 'Pieces',
    info: 'Info',
    privacy: 'Privacy',
    log: 'Log'
  };
  let tab = $state('speed');
  let details = $state<TorrentDetails | null>(null);
  let loadError = $state<string | null>(null);
  let selection = $state<Set<number> | null>(null);
  let canvas = $state<HTMLCanvasElement | null>(null);

  async function load() {
    if (!item.torrent?.infoHash) {
      loadError = 'Waiting for torrent metadata…';
      return;
    }
    try {
      details = await api.torrentDetails(item.id);
      loadError = null;
    } catch (e) {
      loadError = errorText(e);
    }
  }

  onMount(() => {
    load();
    const id = setInterval(load, 1500);
    return () => clearInterval(id);
  });

  let lastId = '';
  $effect(() => {
    if (item.id !== lastId) {
      lastId = item.id;
      details = null;
      selection = null;
      load();
    }
  });

  let history = $derived(app.itemSpeeds[item.id] ?? { down: [], up: [] });
  let ratio = $derived(details && details.progress > 0 ? (details.uploaded / details.progress).toFixed(3) : '0.000');

  let files = $derived(details?.files ?? []);
  let selected = $derived(selection ?? new Set(files.filter((f) => f.included).map((f) => f.index)));
  function toggle(i: number) {
    const s = new Set(selected);
    if (s.has(i)) s.delete(i);
    else s.add(i);
    selection = s;
  }
  async function applyFiles() {
    if (await app.run(() => api.torrentSetFiles(item.id, [...selected]), 'File selection updated')) selection = null;
  }

  let pieces = $derived(details?.pieces ? decodeBits(details.pieces.have, details.pieces.count) : []);
  let havePieces = $derived(pieces.filter(Boolean).length);

  $effect(() => {
    if (tab !== 'pieces' || !canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const w = canvas.clientWidth;
    const cell = pieces.length > 4000 ? 4 : pieces.length > 1500 ? 6 : 9;
    const cols = Math.max(1, Math.floor(w / cell));
    const rows = Math.ceil(pieces.length / cols);
    canvas.width = w;
    canvas.height = Math.max(cell, rows * cell);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    const css = getComputedStyle(document.documentElement);
    const on = css.getPropertyValue('--color-accent').trim() || '#4fc3d1';
    const off = css.getPropertyValue('--color-line').trim() || '#2c2c2e';
    pieces.forEach((have, i) => {
      ctx.fillStyle = have ? on : off;
      ctx.fillRect((i % cols) * cell, Math.floor(i / cols) * cell, cell - 1, cell - 1);
    });
  });

  let s = $derived(app.settings);
  let opt = $state({ down: 0, up: 0, seed: true, ratio: 2 });
  $effect(() => {
    if (s) opt = { down: s.torrentDownloadLimit, up: s.torrentUploadLimit, seed: s.seedAfterComplete, ratio: s.seedRatio };
  });
  async function saveOptions() {
    if (!s) return;
    const ok = await app.saveSettings({
      ...s,
      torrentDownloadLimit: Math.max(0, Number(opt.down) || 0),
      torrentUploadLimit: Math.max(0, Number(opt.up) || 0),
      seedAfterComplete: opt.seed,
      seedRatio: Math.max(0, Number(opt.ratio) || 0)
    });
    if (ok) app.toast('success', t('Save'));
  }

  function copy(text: string) {
    navigator.clipboard.writeText(text).then(() => app.toast('success', t('Copied')));
  }

  const PLAYABLE = /\.(mp4|mkv|webm|avi|mov|m4v|wmv|flv|ts|mp3|flac|m4a|aac|ogg|opus|wav)$/i;

  async function play(index: number) {
    await app.run(async () => api.openUrl(await api.torrentStreamUrl(item.id, index)));
  }

  async function recheck() {
    await app.run(() => api.torrentRecheck(item.id), 'Checking the downloaded data…');
  }

  async function exportTorrent() {
    const dest = await saveDialog({ defaultPath: `${item.name}.torrent`, filters: [{ name: 'Torrent', extensions: ['torrent'] }] });
    if (dest) await app.run(() => api.torrentExport(item.id, dest), 'Torrent file saved');
  }

  const kindLabel: Record<string, string> = { tracker: 'Tracker', dht: 'DHT', lsd: 'LSD', incoming: 'Incoming' };
</script>

<div class="flex items-center justify-between border-b border-line px-2 flex-shrink-0">
  <div class="flex overflow-x-auto">
    {#each tabs as id}
      <button class="tab {tab === id ? 'tab-active' : ''}" onclick={() => (tab = id)}>{t(labels[id])}</button>
    {/each}
  </div>
  <div class="flex items-center gap-1 flex-shrink-0">
    <button class="icon-btn" title={t('Force recheck')} onclick={recheck}><Icon name="refresh" size={13} /></button>
    <button class="icon-btn" title={t('Export .torrent file')} onclick={exportTorrent}><Icon name="file" size={13} /></button>
    <button class="icon-btn" aria-label={t('Close')} onclick={onClose}><Icon name="x" size={13} /></button>
  </div>
</div>

<div class="flex-1 overflow-auto min-h-0 text-[12px] {tab === 'files' || tab === 'sources' || tab === 'peers' ? '' : 'p-4'}">
  {#if loadError && !details && tab !== 'log' && tab !== 'options' && tab !== 'privacy'}
    <p class="text-muted p-4">{t(loadError)}</p>
  {:else if tab === 'speed'}
    <div class="flex gap-6 items-start">
      <div class="rounded-[10px] border border-line bg-bg p-3">
        <SpeedGraph values={history.down} secondary={history.up} width={560} height={150} />
        <div class="flex gap-4 text-[11px] mt-2">
          <span class="text-accent">● {t('Download')}</span><span class="text-seed">● {t('Upload')}</span>
        </div>
      </div>
      <div class="grid grid-cols-[110px_1fr] gap-y-2">
        <span class="text-muted">{t('Download')}</span><span>{speed(item.speed)}</span>
        <span class="text-muted">{t('Upload')}</span><span>{speed(item.uploadSpeed)}</span>
        <span class="text-muted">{t('Downloaded')}</span><span>{bytes(item.downloaded)} / {bytes(item.total)}</span>
        <span class="text-muted">{t('Uploaded')}</span><span>{bytes(item.torrent?.uploaded ?? 0)}</span>
        <span class="text-muted">{t('Ratio')}</span><span>{ratio}</span>
        <span class="text-muted">{t('Peers')}</span><span>{item.peers}</span>
        <span class="text-muted">ETA</span><span>{duration(item.eta)}</span>
        <span class="text-muted">{t('State')}</span><span>{details?.state ?? '—'}{item.seeding ? ' · seeding' : ''}</span>
      </div>
    </div>
  {:else if tab === 'files'}
    <div class="flex items-center gap-2 p-2 border-b border-line">
      <button class="btn !h-[26px]" onclick={() => (selection = new Set(files.map((f) => f.index)))}>{t('Select all')}</button>
      <button class="btn !h-[26px]" onclick={() => (selection = new Set())}>{t('Select none')}</button>
      <button class="btn btn-accent !h-[26px]" disabled={!selection || selected.size === 0} onclick={applyFiles}>{t('Apply')}</button>
      <span class="text-muted ml-2">{selected.size}/{files.length} · {bytes(files.filter((f) => selected.has(f.index)).reduce((a, f) => a + f.length, 0))}</span>
    </div>
    <table class="grid-table">
      <thead><tr><th class="w-[30px]"></th><th>{t('Name')}</th><th>{t('Size')}</th><th>{t('Progress')}</th><th class="w-[90px]"></th></tr></thead>
      <tbody>
        {#each files as f (f.index)}
          <tr>
            <td><input type="checkbox" class="check" checked={selected.has(f.index)} onchange={() => toggle(f.index)} /></td>
            <td title={f.name}>{f.name}</td>
            <td>{bytes(f.length)}</td>
            <td>
              <div class="flex items-center gap-2">
                <div class="w-[120px] h-[4px] bg-line rounded overflow-hidden"><div class="h-full bg-accent" style="width: {percent(f.progress, f.length)}%"></div></div>
                <span class="text-muted">{percent(f.progress, f.length).toFixed(1)}%</span>
              </div>
            </td>
            <td>
              {#if PLAYABLE.test(f.name) && selected.has(f.index)}
                <button class="btn !h-[24px]" title={t('Play while downloading (streams the file in order)')} onclick={() => play(f.index)}><Icon name="play" size={11} />{t('Play')}</button>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {:else if tab === 'options'}
    <div class="grid grid-cols-[230px_1fr] gap-x-4 gap-y-3 max-w-[720px] items-center">
      <span class="text-muted">{t('Global download limit (KB/s)')}</span>
      <input type="number" min="0" class="field !w-[140px]" bind:value={opt.down} />
      <span class="text-muted">{t('Global upload limit (KB/s)')}</span>
      <input type="number" min="0" class="field !w-[140px]" bind:value={opt.up} />
      <span class="text-muted">{t('Keep seeding after completion')}</span>
      <input type="checkbox" class="toggle" bind:checked={opt.seed} />
      <span class="text-muted">{t('Stop seeding at ratio (0 = never)')}</span>
      <input type="number" min="0" step="0.1" class="field !w-[140px]" bind:value={opt.ratio} />
      <span></span>
      <div class="flex gap-2">
        <button class="btn btn-accent" onclick={saveOptions}>{t('Apply')}</button>
        <button class="btn" onclick={() => { app.view = 'settings'; app.settingsSection = 'torrent'; }}>{t('More torrent settings')}</button>
      </div>
    </div>
  {:else if tab === 'sources'}
    <table class="grid-table">
      <thead>
        <tr><th>{t('Type')}</th><th>{t('Details')}</th><th>{t('Status')}</th><th>{t('Peers')}</th><th>{t('Seeds')}</th><th>{t('Leechers')}</th><th>{t('Completed')}</th><th>{t('Next')}</th><th>{t('Interval')}</th></tr>
      </thead>
      <tbody>
        {#each details?.sources ?? [] as row}
          <tr>
            <td>{kindLabel[row.kind] ?? row.kind}</td>
            <td class="mono" title={row.details}>{row.details}</td>
            <td class={row.status.startsWith('Error') ? 'text-bad' : row.status === 'OK' || row.status.startsWith('Online') || row.status === 'Enabled' ? 'text-ok' : 'text-muted'} title={row.status}>{row.status}</td>
            <td>{row.peers ?? ''}</td>
            <td>{row.seeds ?? ''}</td>
            <td>{row.leechers ?? ''}</td>
            <td>{row.completed ?? ''}</td>
            <td>{row.next != null ? duration(row.next) : ''}</td>
            <td>{row.interval != null ? duration(row.interval) : ''}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {:else if tab === 'peers'}
    <table class="grid-table">
      <thead><tr><th>{t('Address')}</th><th>{t('Client')}</th><th>{t('State')}</th><th>{t('Connection')}</th><th>{t('Downloaded')}</th><th>{t('Uploaded')}</th><th>{t('Errors')}</th></tr></thead>
      <tbody>
        {#each details?.peers ?? [] as p (p.addr)}
          <tr>
            <td class="mono">{p.addr}</td>
            <td>{p.client ?? '—'}</td>
            <td>{p.state}</td>
            <td>{p.connection ?? '—'}</td>
            <td>{bytes(p.downloaded)}</td>
            <td>{bytes(p.uploaded)}</td>
            <td>{p.errors}</td>
          </tr>
        {:else}
          <tr><td colspan="7" class="text-muted">{t('No connected peers.')}</td></tr>
        {/each}
      </tbody>
    </table>
  {:else if tab === 'pieces'}
    <div class="flex items-center gap-4 mb-3 text-[11.5px]">
      <span>{havePieces} / {pieces.length} pieces</span>
      {#if details?.info.pieceLength}<span class="text-muted">{t('Piece size')}: {bytes(details.info.pieceLength)}</span>{/if}
      <span class="flex items-center gap-1"><span class="w-[9px] h-[9px] bg-accent inline-block"></span> {t('have')}</span>
      <span class="flex items-center gap-1"><span class="w-[9px] h-[9px] bg-line inline-block"></span> {t('missing')}</span>
    </div>
    <canvas bind:this={canvas} class="w-full"></canvas>
  {:else if tab === 'info' && details}
    <div class="grid grid-cols-[130px_1fr] gap-x-4 gap-y-[7px] max-w-[1000px]">
      <span class="text-muted">{t('Name')}</span><span class="select-text">{details.info.name}</span>
      <span class="text-muted">{t('Info hash')}</span><span class="mono select-text">{details.info.infoHash}</span>
      <span class="text-muted">{t('Size')}</span><span>{bytes(details.info.total)}</span>
      <span class="text-muted">{t('Pieces')}</span><span>{details.info.pieces}{details.info.pieceLength ? ` × ${bytes(details.info.pieceLength)}` : ''}</span>
      <span class="text-muted">{t('Private')}</span><span>{details.info.private ? t('Yes') : t('No')}</span>
      <span class="text-muted">{t('Save path')}</span>
      <span class="flex items-center gap-2"><span class="truncate">{details.info.savePath}</span><button class="icon-btn !w-6 !h-6" onclick={() => app.run(() => api.showInFolder(item.id))}><Icon name="folderOpen" size={12} /></button></span>
      {#if details.info.comment}<span class="text-muted">{t('Comment')}</span><span class="select-text">{details.info.comment}</span>{/if}
      {#if details.info.createdBy}<span class="text-muted">{t('Created by')}</span><span>{details.info.createdBy}</span>{/if}
      {#if details.info.creationDate}<span class="text-muted">{t('Created on')}</span><span>{date(details.info.creationDate * 1000, i18n.lang)}</span>{/if}
      <span class="text-muted">{t('Magnet')}</span>
      <span class="flex items-center gap-2 min-w-0"><span class="truncate mono text-[11px]">{details.info.magnet}</span><button class="btn !h-[24px] flex-shrink-0" onclick={() => copy(details!.info.magnet)}><Icon name="copy" size={12} />{t('Copy magnet link')}</button></span>
      <span class="text-muted">{t('Trackers')}</span>
      <span class="mono text-[11px] select-text whitespace-pre-wrap">{details.info.trackers.join('\n') || '—'}</span>
    </div>
  {:else if tab === 'privacy'}
    {@const priv = details?.info.private ?? false}
    <div class="grid grid-cols-[230px_1fr] gap-x-4 gap-y-[7px] max-w-[900px]">
      <span class="text-muted">{t('Private torrent')}</span><span>{priv ? 'Yes — peers only from trackers' : 'No'}</span>
      <span class="text-muted">DHT</span><span>{!s?.torrentDht ? 'Disabled' : priv ? 'Not used (private)' : 'Enabled'}</span>
      <span class="text-muted">{t('Local Service Discovery')}</span><span>{!s?.torrentLsd ? 'Disabled' : priv ? 'Not used (private)' : 'Enabled'}</span>
      <span class="text-muted">{t('Trackers')}</span><span>{s?.torrentDisableTrackers ? 'Disabled' : 'Enabled'}</span>
      <span class="text-muted">{t('UPnP port mapping')}</span><span>{s?.torrentUpnp ? 'Enabled' : 'Disabled'}</span>
      <span class="text-muted">{t('SOCKS5 proxy')}</span><span>{s?.torrentProxy ? s.torrentProxy.replace(/\/\/.*@/, '//***@') : 'Not used'}</span>
      <span class="text-muted">{t('Bound network interface')}</span><span>{s?.torrentBindInterface || 'Any'}</span>
      <span class="text-muted">{t('IPv4 only')}</span><span>{s?.torrentIpv4Only ? t('Yes') : t('No')}</span>
      <span class="text-muted">{t('IP filter')}</span><span>{s?.ipFilterEnabled ? (s.ipFilterAllowMode ? 'Allow-list active' : 'Block-list active') : 'Off'}</span>
      <span class="text-muted">{t('Client identity')}</span><span>{s?.torrentHideClient ? 'Hidden (generic name)' : 'Multron Download Manager ' + app.version}</span>
      <span></span>
      <button class="btn w-fit" onclick={() => { app.view = 'settings'; app.settingsSection = 'privacy'; }}><Icon name="shield" size={13} />{t('Privacy settings')}</button>
    </div>
  {:else if tab === 'log'}
    <pre class="mono text-[11px] text-muted whitespace-pre-wrap select-text">{(app.logs[item.id] ?? []).join('\n') || 'No events yet.'}{details?.error ? '\nEngine error: ' + details.error : ''}</pre>
  {/if}
</div>
