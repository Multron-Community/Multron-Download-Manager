<script lang="ts">
  import { app } from '../lib/store.svelte';
  import { api } from '../lib/api';
  import { t, i18n } from '../lib/i18n.svelte';
  import { bytes, date, duration, percent, speed } from '../lib/format';
  import type { Download } from '../types';
  import Icon from './Icon.svelte';
  import SpeedGraph from './SpeedGraph.svelte';
  import TorrentTabs from './TorrentTabs.svelte';

  interface Props {
    item: Download;
  }
  let { item }: Props = $props();

  let tab = $state('general');
  let lastKind = $state('');
  $effect(() => {
    if (item.kind !== lastKind) {
      lastKind = item.kind;
      tab = item.kind === 'torrent' ? 'speed' : 'general';
    }
  });

  let tabs = $derived(
    item.kind === 'http'
      ? ['general', 'speed', 'segments', 'integrity', 'log']
      : item.kind === 'media'
        ? ['general', 'speed', 'files', 'log']
        : []
  );
  const tabLabels: Record<string, string> = {
    general: 'General',
    speed: 'Speed',
    segments: 'Segments',
    integrity: 'Integrity',
    files: 'Files',
    log: 'Log'
  };

  let threads = $state(8);
  let limit = $state(0);
  let hashInput = $state('');
  let lastId = $state('');
  $effect(() => {
    if (item.id !== lastId) {
      lastId = item.id;
      threads = item.threads;
      limit = item.speedLimit;
      hashInput = item.sha256Expected ?? '';
    }
  });

  let history = $derived(app.itemSpeeds[item.id] ?? { down: [], up: [] });
  let peak = $derived(Math.max(0, ...history.down));
  let avg = $derived(history.down.length ? history.down.reduce((a, b) => a + b, 0) / history.down.length : 0);

  function copy(text: string) {
    navigator.clipboard.writeText(text).then(() => app.toast('success', t('Copied')));
  }
</script>

<section class="h-[300px] border-t border-line bg-panel flex flex-col flex-shrink-0 min-h-0">
  {#if item.kind === 'torrent'}
    <TorrentTabs {item} onClose={() => (app.selectedId = null)} />
  {:else}
    <div class="flex items-center justify-between border-b border-line px-2 flex-shrink-0">
      <div class="flex overflow-x-auto">
        {#each tabs as id}
          <button class="tab {tab === id ? 'tab-active' : ''}" onclick={() => (tab = id)}>{t(tabLabels[id])}</button>
        {/each}
      </div>
      <button class="icon-btn" aria-label={t('Close')} onclick={() => (app.selectedId = null)}><Icon name="x" size={13} /></button>
    </div>

    <div class="flex-1 overflow-auto p-4 min-h-0 text-[12px]">
      {#if tab === 'general'}
        <div class="grid grid-cols-[130px_1fr] gap-x-4 gap-y-[7px] max-w-[980px]">
          <span class="text-muted">URL</span>
          <span class="flex items-center gap-2 min-w-0">
            <span class="truncate mono text-[11.5px]" title={item.url}>{item.url}</span>
            <button class="icon-btn !w-6 !h-6 flex-shrink-0" title={t('Copy')} onclick={() => copy(item.url)}><Icon name="copy" size={12} /></button>
          </span>
          <span class="text-muted">{t('Save path')}</span>
          <span class="flex items-center gap-2 min-w-0">
            <span class="truncate" title={item.dir}>{item.dir}</span>
            <button class="icon-btn !w-6 !h-6 flex-shrink-0" onclick={() => app.run(() => api.showInFolder(item.id))}><Icon name="folderOpen" size={12} /></button>
          </span>
          <span class="text-muted">{t('Size')}</span>
          <span>{bytes(item.downloaded)} / {bytes(item.total)} ({percent(item.downloaded, item.total).toFixed(1)}%){item.kind === 'http' ? ' · ' + (item.resumable ? t('Resumable') : t('Not resumable')) : ''}</span>
          <span class="text-muted">{t('Speed')}</span>
          <span>{speed(item.speed)} · ETA {duration(item.eta)}</span>
          <span class="text-muted">{t('Connections')}</span>
          <span class="flex items-center gap-3">
            <input type="range" min="1" max="64" bind:value={threads} class="w-[220px] accent-[var(--color-accent-strong)]" onchange={() => app.run(() => api.setThreads(item.id, threads))} />
            <span class="w-[110px]">{threads} {item.status === 'active' && item.kind === 'http' ? `(${item.connections} open)` : ''}</span>
          </span>
          <span class="text-muted">{t('Speed limit')}</span>
          <span class="flex items-center gap-2">
            <input type="number" min="0" class="field !w-[110px] !h-[26px]" bind:value={limit} onchange={() => app.run(() => api.setSpeedLimit(item.id, Math.max(0, Number(limit) || 0)))} />
            <span class="text-muted">{t('KB/s (0 = unlimited)')}</span>
          </span>
          {#if item.media}
            <span class="text-muted">{t('Quality')}</span>
            <span>{item.media.quality}{item.media.playlist ? ' · playlist' : ''}</span>
          {/if}
          <span class="text-muted">{t('Added')}</span>
          <span>{date(item.createdAt, i18n.lang)}{item.completedAt ? ' · ' + t('Finished {date}', { date: date(item.completedAt, i18n.lang) }) : ''}</span>
          {#if item.error}
            <span class="text-muted">{t('Error')}</span>
            <span class="text-bad whitespace-pre-wrap select-text">{item.error}</span>
          {/if}
        </div>
      {:else if tab === 'speed'}
        <div class="flex gap-6 items-start">
          <div class="rounded-[10px] border border-line bg-bg p-3">
            <SpeedGraph values={history.down} width={560} height={150} />
          </div>
          <div class="grid grid-cols-[90px_1fr] gap-y-2 text-[12px]">
            <span class="text-muted">{t('Current')}</span><span>{speed(item.speed)}</span>
            <span class="text-muted">{t('Average')}</span><span>{speed(avg)}</span>
            <span class="text-muted">{t('Peak')}</span><span>{speed(peak)}</span>
            <span class="text-muted">{t('Connections')}</span><span>{item.connections}</span>
            <span class="text-muted">ETA</span><span>{duration(item.eta)}</span>
          </div>
        </div>
      {:else if tab === 'segments'}
        {#if item.total && item.segments.length}
          <div class="relative w-full h-[18px] bg-line rounded-[4px] overflow-hidden mb-3">
            {#each item.segments as s}
              <div class="absolute top-0 h-full bg-accent/85 border-r border-bg" style="left: {(s.start / item.total) * 100}%; width: {((s.pos - s.start) / item.total) * 100}%"></div>
            {/each}
          </div>
          <table class="grid-table">
            <thead><tr><th>#</th><th>{t('Start')}</th><th>{t('End')}</th><th>{t('Downloaded')}</th><th>{t('Progress')}</th></tr></thead>
            <tbody>
              {#each item.segments as s, i}
                <tr>
                  <td>{i + 1}</td>
                  <td class="mono">{s.start}</td>
                  <td class="mono">{s.end}</td>
                  <td>{bytes(s.pos - s.start)} / {bytes(s.end - s.start)}</td>
                  <td>
                    <div class="w-[160px] h-[4px] bg-line rounded overflow-hidden">
                      <div class="h-full bg-accent" style="width: {percent(s.pos - s.start, s.end - s.start)}%"></div>
                    </div>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        {:else}
          <p class="text-muted">{t('This download uses a single connection (the server does not support byte ranges or has not been probed yet).')}</p>
        {/if}
      {:else if tab === 'integrity'}
        <div class="max-w-[760px] space-y-3">
          <div class="space-y-1.5">
            <span class="label">{t('Expected SHA-256')}</span>
            <div class="flex gap-2">
              <input class="field mono" placeholder="e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855" bind:value={hashInput} />
              <button class="btn btn-accent" onclick={() => app.run(() => api.setExpectedHash(item.id, hashInput.trim() || null))}>
                {item.status === 'completed' || item.status === 'failed' ? 'Verify' : t('Save')}
              </button>
            </div>
          </div>
          <div class="grid grid-cols-[130px_1fr] gap-y-2">
            <span class="text-muted">{t('State')}</span>
            <span class={item.hashState === 'ok' ? 'text-ok' : item.hashState === 'mismatch' ? 'text-bad' : ''}>
              {t(({ none: 'Not checked', pending: 'Will be verified when the download finishes', verifying: 'Verifying…', ok: 'Verified ✓', mismatch: 'Mismatch ✗', computed: 'Computed' } as Record<string, string>)[item.hashState])}
            </span>
            <span class="text-muted">{t('Actual SHA-256')}</span>
            <span class="mono text-[11.5px] select-text break-all">{item.sha256Actual ?? '—'}</span>
          </div>
        </div>
      {:else if tab === 'files'}
        {#if item.media?.files.length}
          <table class="grid-table">
            <thead><tr><th>{t('Files')}</th><th></th></tr></thead>
            <tbody>
              {#each item.media.files as f}
                <tr><td class="mono" title={f}>{f}</td><td><button class="icon-btn !w-6 !h-6" onclick={() => app.run(() => api.openPath(f))}><Icon name="file" size={12} /></button></td></tr>
              {/each}
            </tbody>
          </table>
        {:else}
          <p class="text-muted">{t('Files appear here once yt-dlp has written them.')}</p>
        {/if}
      {:else if tab === 'log'}
        <pre class="mono text-[11px] text-muted whitespace-pre-wrap select-text">{(app.logs[item.id] ?? []).join('\n') || 'No events yet.'}</pre>
      {/if}
    </div>
  {/if}
</section>
