<script lang="ts">
  import type { Download } from '../types';
  import { app } from '../lib/store.svelte';
  import { api } from '../lib/api';
  import { t, tc } from '../lib/i18n.svelte';
  import { bytes, duration, extension, percent, speed } from '../lib/format';
  import Icon from './Icon.svelte';

  interface Props {
    item: Download;
  }
  let { item }: Props = $props();

  let tool = $derived(item.status === 'active' && item.toolName ? item.toolName : null);
  let pct = $derived(tool ? percent(item.toolDownloaded, item.toolTotal) : percent(item.downloaded, item.total));
  let barTotal = $derived(tool ? item.toolTotal : item.total);
  let checked = $derived(app.checked.includes(item.id));
  let selected = $derived(app.selectedId === item.id || checked);
  let ext = $derived(
    item.kind === 'torrent'
      ? 'TOR'
      : /^https?:/i.test(item.name)
        ? item.kind === 'media'
          ? 'VID'
          : 'URL'
        : extension(item.name).slice(0, 4) || item.kind.slice(0, 3)
  );

  const phaseLabel: Record<string, string> = {
    connecting: 'Connecting',
    downloading: 'Downloading',
    verifying: 'Verifying',
    merging: 'Merging',
    converting: 'Converting',
    tagging: 'Converting',
    metadata: 'Fetching metadata',
    checking: 'Checking',
    preparing: 'Preparing',
    'installing FFmpeg': 'Installing FFmpeg',
    'extracting FFmpeg': 'Extracting FFmpeg',
    'waiting for network': 'Waiting for the network',
    retrying: 'Reconnecting'
  };

  let statusText = $derived.by(() => {
    if (item.status === 'active' && item.phase && phaseLabel[item.phase]) return t(phaseLabel[item.phase]);
    if (item.status === 'completed' && item.seeding) return t('Seeding');
    if (item.status === 'active') return t('Downloading');
    if (item.status === 'queued') return tc('status', 'Queued');
    if (item.status === 'paused') return tc('status', 'Paused');
    if (item.status === 'completed') return tc('status', 'Completed');
    if (item.status === 'failed') return tc('status', 'Failed');
    return item.status;
  });

  let statusColor = $derived(
    item.status === 'failed'
      ? 'text-bad'
      : item.status === 'completed'
        ? item.seeding
          ? 'text-seed'
          : 'text-ok'
        : item.status === 'active'
          ? 'text-white'
          : 'text-muted'
  );

  const kindIcon: Record<string, string> = { http: 'link', media: 'film', torrent: 'magnet' };

  const speedPresets = [0, 256, 512, 1024, 2048, 5120, 10240];
  let speedMenu = $state(false);

  function limitLabel(kib: number) {
    return kib >= 1024 ? `${+(kib / 1024).toFixed(1)} MB/s` : `${kib} KB/s`;
  }

  async function setLimit(kib: number) {
    speedMenu = false;
    await app.run(() => api.setSpeedLimit(item.id, kib));
  }

  function move(e: MouseEvent, to: 'up' | 'down' | 'top' | 'bottom') {
    e.stopPropagation();
    app.run(() => api.moveInQueue(item.id, to));
  }

  function explain(e: MouseEvent, kind: 'info' | 'success' | 'warning' | 'error', text: string) {
    e.stopPropagation();
    app.toast(kind, text);
  }

  const PROGRAM = /\.(exe|msi|msix|msixbundle|appx|appxbundle|bat|cmd|com|scr|pif|ps1|vbs|vbe|js|jse|wsf|hta|cpl|lnk|reg|jar|appimage|run|sh|deb|rpm|pkg|dmg)$/i;

  function openOnDoubleClick() {
    if (item.status !== 'completed') return;
    if (PROGRAM.test(item.name) && !app.settings?.openProgramsOnDoubleClick) {
      app.toast('info', t('Programs are not started with a double-click. Use the open button, or turn this on in Options › Security.'));
      return;
    }
    app.run(() => api.openFile(item.id));
  }

  async function primary(e: MouseEvent) {
    e.stopPropagation();
    if (item.status === 'active' || item.status === 'queued') await app.run(() => api.pause(item.id));
    else if (item.status === 'completed') {
      if (item.kind === 'torrent' && item.seeding) await app.run(() => api.pause(item.id));
      else await app.run(() => api.openFile(item.id));
    } else await app.run(() => api.resume(item.id));
  }
</script>

<svelte:window onclick={() => (speedMenu = false)} />

<div
  role="button"
  tabindex="0"
  onclick={(e) => app.pick(item.id, e)}
  onmousedown={(e) => e.shiftKey && e.preventDefault()}
  onkeydown={(e) => e.key === 'Enter' && (app.selectedId = item.id)}
  ondblclick={openOnDoubleClick}
  class="w-full p-[12px_14px] border rounded-[12px] transition-all group flex flex-col gap-[10px] cursor-default outline-none
    {selected ? 'border-accent/50 bg-raised' : item.status === 'active' ? 'border-line bg-panel' : 'border-card-line bg-bg hover:bg-subtle'}"
>
  <div class="flex items-center justify-between gap-3">
    <div class="flex items-center gap-[12px] min-w-0 flex-1">
      <div lang="en" class="relative w-[38px] h-[38px] rounded-[9px] bg-line flex items-center justify-center flex-shrink-0 font-bold text-[9.5px] uppercase tracking-wider overflow-hidden">
        {#if item.media?.thumbnail}
          <img src={item.media.thumbnail} alt="" class="absolute inset-0 w-full h-full object-cover opacity-80" />
        {:else}
          {ext}
        {/if}
        {#if checked}
          <span class="absolute inset-0 bg-accent-strong/85 flex items-center justify-center text-white"><Icon name="check" size={18} stroke={3} /></span>
        {/if}
        <span class="absolute -bottom-[1px] -right-[1px] w-[15px] h-[15px] rounded-[5px] bg-bg flex items-center justify-center text-muted">
          <Icon name={kindIcon[item.kind]} size={10} />
        </span>
      </div>

      <div class="flex flex-col gap-[4px] min-w-0 flex-1">
        <h3 class="text-[13.5px] font-medium truncate leading-tight" title={item.name}>{item.name}</h3>
        <div class="flex items-center gap-[7px] text-[11px] leading-tight text-muted whitespace-nowrap overflow-hidden">
          {#if tool}
            <span class="text-accent">{tool}: {bytes(item.toolDownloaded)}{item.toolTotal ? ` / ${bytes(item.toolTotal)}` : ''}</span>
          {:else}
            <span>{bytes(item.downloaded)}{item.total ? ` / ${bytes(item.total)}` : ''}</span>
          {/if}
          <span class="w-[3px] h-[3px] rounded-full bg-dim"></span>
          <span class={statusColor}>{statusText}</span>
          {#if item.status === 'queued' && item.queuePos}
            <span class="text-accent" title={t('Place in the queue')}>#{item.queuePos}</span>
          {/if}
          {#if item.speedLimit > 0 && item.kind !== 'torrent'}
            <span class="flex items-center gap-1 text-warn" title={t('Speed limit for this download')}><Icon name="gauge" size={11} />≤ {limitLabel(item.speedLimit)}</span>
          {/if}
          {#if item.status === 'active' || item.status === 'paused'}
            <span class="w-[3px] h-[3px] rounded-full bg-dim"></span>
            <span class="text-white">{pct.toFixed(1)}%</span>
          {/if}
          {#if item.itemCount && item.itemCount > 1}
            <span class="w-[3px] h-[3px] rounded-full bg-dim"></span>
            <span>{item.itemIndex ?? 1}/{item.itemCount}</span>
          {/if}
          {#if item.status === 'active' && item.connections > 0}
            <span class="w-[3px] h-[3px] rounded-full bg-dim"></span>
            <span title={item.kind === 'torrent' ? t('Peers') : t('Connections')}>
              {item.kind === 'torrent' ? t('{count} peers', { count: item.connections }) : t('{active}/{total} conn.', { active: item.connections, total: item.threads })}
            </span>
          {/if}
          {#if item.scanState === 'threat'}
            <button type="button" class="flex items-center gap-1 text-bad hover:underline cursor-pointer" title={t('Click to see why')} onclick={(e) => explain(e, 'error', t('Antivirus') + ': ' + (item.scanDetail || t('No details')))} ondblclick={(e) => e.stopPropagation()}><Icon name="alert" size={11} />{t('Threat found')}</button>
          {:else if item.scanState === 'clean'}
            <button type="button" class="flex items-center gap-1 text-ok hover:underline cursor-pointer" title={t('Click to see why')} onclick={(e) => explain(e, 'success', t('Antivirus') + ': ' + (item.scanDetail || t('No details')))} ondblclick={(e) => e.stopPropagation()}><Icon name="shield" size={11} />{t('Scanned')}</button>
          {:else if item.scanState === 'scanning'}
            <span class="flex items-center gap-1 text-dim"><Icon name="shield" size={11} />{t('Scanning…')}</span>
          {:else if item.scanState === 'skipped'}
            <button type="button" class="flex items-center gap-1 text-warn hover:underline cursor-pointer" title={t('Click to see why')} onclick={(e) => explain(e, 'warning', t('Antivirus') + ': ' + (item.scanDetail || t('No details')))} ondblclick={(e) => e.stopPropagation()}><Icon name="alert" size={11} />{t('Not scanned')}</button>
          {/if}
          {#if item.cloudState === 'threat'}
            <button type="button" class="flex items-center gap-1 text-bad hover:underline cursor-pointer" title={t('Click to see why')} onclick={(e) => explain(e, 'error', item.cloudDetail || t('No details'))} ondblclick={(e) => e.stopPropagation()}><Icon name="alert" size={11} />{t('Multron Cloud: threat')}<span class="px-1 rounded bg-warn/15 text-warn text-[9.5px] font-semibold uppercase tracking-wide">{t('Testing')}</span></button>
          {:else if item.cloudState === 'suspicious'}
            <button type="button" class="flex items-center gap-1 text-warn hover:underline cursor-pointer" title={t('Click to see why')} onclick={(e) => explain(e, 'warning', item.cloudDetail || t('No details'))} ondblclick={(e) => e.stopPropagation()}><Icon name="alert" size={11} />{t('Multron Cloud: suspicious')}<span class="px-1 rounded bg-warn/15 text-warn text-[9.5px] font-semibold uppercase tracking-wide">{t('Testing')}</span></button>
          {:else if item.cloudState === 'clean'}
            <button type="button" class="flex items-center gap-1 text-ok hover:underline cursor-pointer" title={t('Click to see why')} onclick={(e) => explain(e, 'success', t('Scanned separately by Multron Cloud'))} ondblclick={(e) => e.stopPropagation()}><Icon name="shield" size={11} />{t('Multron Cloud: clean')}<span class="px-1 rounded bg-warn/15 text-warn text-[9.5px] font-semibold uppercase tracking-wide">{t('Testing')}</span></button>
          {:else if item.cloudState === 'scanning'}
            <span class="flex items-center gap-1 text-dim"><Icon name="shield" size={11} />{t('Multron Cloud: scanning…')}<span class="px-1 rounded bg-warn/15 text-warn text-[9.5px] font-semibold uppercase tracking-wide">{t('Testing')}</span></span>
          {:else if item.cloudState === 'skipped' || item.cloudState === 'error'}
            <button type="button" class="flex items-center gap-1 text-warn hover:underline cursor-pointer" title={t('Click to see why')} onclick={(e) => explain(e, 'warning', item.cloudDetail || t('No details'))} ondblclick={(e) => e.stopPropagation()}><Icon name="alert" size={11} />{t('Multron Cloud: not scanned')}<span class="px-1 rounded bg-warn/15 text-warn text-[9.5px] font-semibold uppercase tracking-wide">{t('Testing')}</span></button>
          {/if}
          {#if item.hashState === 'ok'}
            <span class="flex items-center gap-1 text-ok" title={item.sha256Actual ?? ''}><Icon name="shield" size={11} />SHA-256</span>
          {:else if item.hashState === 'mismatch'}
            <span class="flex items-center gap-1 text-bad"><Icon name="alert" size={11} />SHA-256</span>
          {:else if item.hashState === 'pending'}
            <span class="flex items-center gap-1 text-dim" title={item.sha256Expected ?? ''}><Icon name="hash" size={11} />SHA-256</span>
          {/if}
          {#if item.error && item.status === 'failed'}
            <span class="w-[3px] h-[3px] rounded-full bg-dim"></span>
            <span class="text-bad truncate" title={item.error}>{item.error}</span>
          {/if}
        </div>
      </div>
    </div>

    <div class="flex items-center gap-[12px] flex-shrink-0">
      {#if (item.status === 'active' && item.speed > 0) || item.uploadSpeed > 0}
        <div class="flex flex-col items-end gap-[2px] leading-tight text-right">
          <span class="text-[12.5px] font-medium">{speed(item.speed)}</span>
          <span class="text-[10.5px] text-muted">
            {#if item.uploadSpeed > 0}↑ {speed(item.uploadSpeed)}{:else}{duration(item.eta)}{/if}
          </span>
        </div>
      {/if}

      <div class="flex items-center gap-1 transition-opacity {speedMenu ? 'opacity-100' : 'opacity-0 group-hover:opacity-100'}">
        {#if item.status === 'queued' && item.queuePos}
          <button class="icon-btn" title={t('Move up in the queue')} onclick={(e) => move(e, 'up')} ondblclick={(e) => move(e, 'top')}>
            <Icon name="chevronUp" size={14} />
          </button>
          <button class="icon-btn" title={t('Move down in the queue')} onclick={(e) => move(e, 'down')} ondblclick={(e) => move(e, 'bottom')}>
            <Icon name="chevronDown" size={14} />
          </button>
        {/if}
        {#if item.kind !== 'torrent' && item.status !== 'completed'}
          <div class="relative">
            <button class="icon-btn {item.speedLimit > 0 ? '!text-warn' : ''}" title={t('Speed limit for this download')} onclick={(e) => { e.stopPropagation(); speedMenu = !speedMenu; }}>
              <Icon name="gauge" size={14} />
            </button>
            {#if speedMenu}
              <div
                role="menu"
                tabindex="-1"
                class="absolute right-0 top-[32px] z-30 w-[170px] bg-menu border border-line rounded-[10px] shadow-xl p-1"
                onclick={(e) => e.stopPropagation()}
                onkeydown={(e) => e.key === 'Escape' && (speedMenu = false)}
              >
                <div class="px-2 py-1 text-[10.5px] text-dim uppercase tracking-wider">{t('Speed limit')}</div>
                {#each speedPresets as kib}
                  <button
                    class="w-full h-[28px] px-2 rounded-[6px] text-left text-[12px] flex items-center justify-between cursor-pointer hover:bg-hover {item.speedLimit === kib ? 'text-accent' : ''}"
                    onclick={() => setLimit(kib)}
                  >
                    {kib === 0 ? t('Unlimited') : limitLabel(kib)}
                    {#if item.speedLimit === kib}<Icon name="check" size={12} />{/if}
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        {/if}
        {#if item.status === 'completed'}
          <button class="icon-btn" title={t('Show in folder')} onclick={(e) => { e.stopPropagation(); app.run(() => api.showInFolder(item.id)); }}>
            <Icon name="folderOpen" size={14} />
          </button>
        {/if}
        <button class="icon-btn hover:!text-bad" title={t('Remove')} onclick={(e) => { e.stopPropagation(); app.askRemove([item]); }}>
          <Icon name="trash" size={14} />
        </button>
      </div>

      <button
        type="button"
        onclick={primary}
        class="w-[30px] h-[30px] rounded-full bg-line hover:bg-btn-hover flex items-center justify-center cursor-pointer active:scale-95"
        title={item.status === 'active' || item.status === 'queued' ? t('Pause') : item.status === 'completed' ? (item.seeding ? t('Pause') : t('Open')) : item.status === 'failed' ? t('Retry') : t('Resume')}
      >
        {#if item.status === 'active' || item.status === 'queued' || (item.status === 'completed' && item.seeding)}
          <Icon name="pause" size={12} />
        {:else if item.status === 'completed'}
          <Icon name="file" size={13} />
        {:else if item.status === 'failed'}
          <Icon name="refresh" size={13} />
        {:else}
          <Icon name="play" size={12} class="ml-[2px]" />
        {/if}
      </button>
    </div>
  </div>

  {#if item.status !== 'completed'}
    <div class="relative w-full h-[5px] bg-line rounded-[3px] overflow-hidden">
      <div
        class="h-full rounded-[3px] transition-[width] duration-300 {item.status === 'active' ? 'bg-accent' : item.status === 'failed' ? 'bg-bad/60' : 'bg-dim'} {item.status === 'active' && !barTotal ? 'progress-live w-full' : ''}"
        style={barTotal ? `width: ${pct}%` : ''}
      ></div>
    </div>
  {/if}
</div>
