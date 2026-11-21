<script lang="ts">
  import type { Download } from '../types';
  import { app, PROGRAM_FILE } from '../lib/store.svelte';
  import { api } from '../lib/api';
  import { t, tc } from '../lib/i18n.svelte';
  import { bytes, duration, extension, percent, speed } from '../lib/format';
  import Icon from './Icon.svelte';
  import { cloudView } from '../lib/cloud';
  import { tagStyle, signatureView } from '../lib/tags';

  interface Props {
    item: Download;
  }
  let { item }: Props = $props();

  let tool = $derived(item.status === 'active' && item.toolName ? item.toolName : null);
  let pct = $derived(tool ? percent(item.toolDownloaded, item.toolTotal) : percent(item.downloaded, item.total));
  let barTotal = $derived(tool ? item.toolTotal : item.total);
  let checked = $derived(app.checked.includes(item.id));
  let flash = $state<'' | 'done' | 'fail'>('');
  let lastStatus: string | null = null;
  let flashTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    const now = item.status;
    if (lastStatus && lastStatus !== now && (now === 'completed' || now === 'failed')) {
      flash = now === 'completed' ? 'done' : 'fail';
      clearTimeout(flashTimer);
      flashTimer = setTimeout(() => (flash = ''), 1400);
    }
    lastStatus = now;
  });
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
    tagging: 'Adding metadata',
    thumbnail: 'Adding the thumbnail',
    finishing: 'Finishing',
    metadata: 'Fetching metadata',
    checking: 'Checking',
    preparing: 'Preparing',
    'installing FFmpeg': 'Installing FFmpeg',
    'extracting FFmpeg': 'Extracting FFmpeg',
    'installing Deno': 'Installing Deno',
    'extracting Deno': 'Extracting Deno',
    'waiting for network': 'Waiting for the network',
    retrying: 'Reconnecting',
    'waiting for host': 'Waiting (the site asks to wait)',
    'refreshing link': 'Refreshing the link'
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

  const cloud = $derived(cloudView(item.cloudState, item.cloudDetail));
  const sigView = $derived(signatureView(item.signature));

  async function explainCloud(e: MouseEvent, kind: 'info' | 'success' | 'warning' | 'error', text: string) {
    e.stopPropagation();
    if (await app.showCloudReport(item)) return;
    app.toast(kind, text);
  }

  function openOnDoubleClick() {
    if (item.status !== 'completed' || app.confirmingOpen) return;
    app.openDownload(item);
  }

  function scanInCloud(e: MouseEvent) {
    e.stopPropagation();
    if (!app.settings?.cloudScanAccepted) {
      app.cloudEulaFor = item.id;
      return;
    }
    app.scanInCloud(item);
  }

  async function primary(e: MouseEvent) {
    e.stopPropagation();
    if (item.status === 'active' || item.status === 'queued') await app.run(() => api.pause(item.id));
    else if (item.status === 'completed') {
      if (item.kind === 'torrent' && item.seeding) await app.run(() => api.pause(item.id));
      else app.openDownload(item);
    } else await app.run(() => api.resume(item.id));
  }

  let dragFrom: { x: number; y: number } | null = null;
  let dragged = false;
  function dragDown(e: PointerEvent) {
    dragged = false;
    dragFrom = null;
    if (e.button !== 0 || e.shiftKey || e.ctrlKey || e.metaKey || item.status !== 'completed' || app.platform !== 'windows') return;
    if ((e.target as Element | null)?.closest('button, a, input, select, textarea')) return;
    dragFrom = { x: e.clientX, y: e.clientY };
  }
  function dragMove(e: PointerEvent) {
    if (!dragFrom) return;
    if ((e.buttons & 1) === 0) {
      dragFrom = null;
      return;
    }
    if (e.type === 'pointermove' && Math.hypot(e.clientX - dragFrom.x, e.clientY - dragFrom.y) < 6) return;
    dragFrom = null;
    dragged = true;
    window.getSelection()?.removeAllRanges();
    const ids = app.checked.includes(item.id) ? app.checkedItems.filter((d) => d.status === 'completed').map((d) => d.id) : [item.id];
    api
      .dragOut(ids)
      .then((n) => n === 0 && app.toast('warning', t('This file cannot be dragged out: it is missing, in quarantine or marked as a threat.')))
      .catch((err) => app.toast('error', String(err)));
  }
</script>

<svelte:window onclick={() => (speedMenu = false)} />

<div
  role="button"
  tabindex="0"
  onclick={(e) => {
    if (dragged) {
      dragged = false;
      return;
    }
    app.pick(item.id, e);
  }}
  onmousedown={(e) => e.shiftKey && e.preventDefault()}
  onpointerdown={dragDown}
  onpointermove={dragMove}
  onpointerleave={dragMove}
  ondragstart={(e) => item.status === 'completed' && e.preventDefault()}
  onkeydown={(e) => e.key === 'Enter' && (app.selectedId = item.id)}
  ondblclick={openOnDoubleClick}
  data-state={item.cloudState === 'threat' || item.quarantined ? 'threat' : item.status}
  class="download-card card-enter {flash === 'done' ? 'flash-done' : flash === 'fail' ? 'flash-fail' : ''} w-full p-[12px_14px] border rounded-[12px] transition-all group flex flex-col gap-[10px] cursor-default outline-none
    {selected ? 'border-accent/50 bg-raised' : item.status === 'active' ? 'border-line bg-panel' : 'border-card-line bg-bg hover:bg-subtle'}"
>
  {#if item.status === 'active' && barTotal}
    <div class="card-fill" style="width: {Math.min(100, pct)}%" aria-hidden="true"></div>
  {/if}
  <div class="flex items-center justify-between gap-3">
    <div class="flex items-center gap-[12px] min-w-0 flex-1">
      <div lang="en" data-cat={item.category} class="file-badge {item.status === 'active' ? 'is-active' : ''} relative w-[38px] h-[38px] rounded-[10px] flex items-center justify-center flex-shrink-0 font-bold text-[9.5px] uppercase tracking-wider overflow-hidden">
        {#if item.media?.thumbnail}
          <img src={item.media.thumbnail} alt="" class="absolute inset-0 w-full h-full object-cover opacity-80" />
        {:else}
          {ext}
        {/if}
        {#if checked}
          <span class="status-pop absolute inset-0 bg-accent-strong/85 flex items-center justify-center text-white"><Icon name="check" size={18} stroke={3} /></span>
        {/if}
        <span class="absolute -bottom-[1px] -right-[1px] w-[15px] h-[15px] rounded-[5px] bg-bg flex items-center justify-center text-muted">
          <Icon name={kindIcon[item.kind]} size={10} />
        </span>
      </div>

      <div class="flex flex-col gap-[4px] min-w-0 flex-1">
        <h3 class="text-[13.5px] font-medium truncate leading-tight" title={item.name}>{#if app.disguised(item)}<span class="pill bg-bad/15 text-bad mr-1.5 align-middle" title={t('The name of this file hides that it is a program, for example “invoice.pdf.exe”. This is a common trick of malware.')}>{t('Disguised program')}</span>{/if}{item.name}</h3>
        <div class="flex flex-wrap items-center gap-x-[7px] gap-y-1 text-[11px] leading-tight text-muted whitespace-nowrap">
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
            <button type="button" class="flex items-center gap-1 text-bad hover:underline cursor-pointer" title={t('Click to see why')} onclick={(e) => explain(e, 'error', t('Antivirus') + ': ' + (item.scanDetail || t('No details')))} ondblclick={(e) => e.stopPropagation()}><Icon name="alert" size={11} />{t('Blocked by your antivirus')}</button>
          {:else if item.scanState === 'marked'}
            {@const shield = app.platform === 'macos' ? 'Gatekeeper' : 'SmartScreen'}
            <button type="button" class="flex items-center gap-1 text-ok hover:underline cursor-pointer" title={t('Marked as downloaded from the internet, so {name} checks the file when it is opened.', { name: shield })} onclick={(e) => explain(e, 'success', t('Marked as downloaded from the internet, so {name} checks the file when it is opened.', { name: shield }))} ondblclick={(e) => e.stopPropagation()}><Icon name="shield" size={11} />{shield}</button>
          {/if}
          {#if item.cloudState === 'scanning'}
            <span class="flex items-center gap-1 text-accent font-medium cursor-pointer hover:underline" role="button" tabindex="-1" onclick={(e) => { e.stopPropagation(); app.showCloudReport(item); }} onkeydown={() => {}}><Icon name="shield" size={11} />{t('Multron Cloud: scanning…')}<span class="px-1 rounded bg-warn/15 text-warn text-[9.5px] font-semibold uppercase tracking-wide">{t('Testing')}</span></span>
          {:else if item.cloudState === 'locked'}
            <button type="button" class="flex items-center gap-1 text-warn hover:underline cursor-pointer" title={cloud.reason} onclick={(e) => { e.stopPropagation(); app.cloudPasswordFor = item.id; }} ondblclick={(e) => e.stopPropagation()}><Icon name="lock" size={11} />{cloud.badge}<span class="px-1 rounded bg-warn/15 text-warn text-[9.5px] font-semibold uppercase tracking-wide">{t('Testing')}</span></button>
          {:else if item.cloudState}
            <button type="button" class="flex items-center gap-1 {cloud.tone === 'bad' ? 'text-bad' : cloud.tone === 'ok' ? 'text-ok' : 'text-warn'} hover:underline cursor-pointer" title={cloud.reason || t('Click to see why')} onclick={(e) => explainCloud(e, cloud.tone === 'bad' ? 'error' : cloud.tone === 'ok' ? 'success' : 'warning', item.cloudState === 'clean' ? t('Scanned separately by Multron Cloud') : cloud.reason || t('No details'))} ondblclick={(e) => e.stopPropagation()}><Icon name={cloud.icon === 'shield' ? 'shield' : 'alert'} size={11} />{cloud.badge}<span class="px-1 rounded bg-warn/15 text-warn text-[9.5px] font-semibold uppercase tracking-wide">{t('Testing')}</span></button>
          {:else if item.status === 'completed' && (app.settings?.cloudScanButton !== false || app.settings?.cloudScan)}
            <button type="button" class="flex items-center gap-1 text-muted font-medium hover:underline cursor-pointer" title={t('Scan for malware with Multron Cloud')} onclick={scanInCloud} ondblclick={(e) => e.stopPropagation()}><Icon name="shield" size={11} />{t('Multron Cloud: not scanned')}<span class="px-1 rounded bg-warn/15 text-warn text-[9.5px] font-semibold uppercase tracking-wide">{t('Testing')}</span></button>
          {/if}
          {#if item.hashState === 'ok'}
            <span class="flex items-center gap-1 text-ok" title={item.sha256Actual ?? ''}><Icon name="shield" size={11} />SHA-256</span>
          {:else if item.hashState === 'mismatch'}
            <span class="flex items-center gap-1 text-bad"><Icon name="alert" size={11} />SHA-256</span>
          {:else if item.hashState === 'pending'}
            <span class="flex items-center gap-1 text-dim" title={item.sha256Expected ?? ''}><Icon name="hash" size={11} />SHA-256</span>
          {/if}
          {#each item.tags ?? [] as tag (tag)}
            <button type="button" class="h-[17px] px-1.5 rounded-full border text-[10.5px] leading-none cursor-pointer" style={tagStyle(tag)} onclick={(e) => { e.stopPropagation(); app.tagFilter = app.tagFilter === tag ? null : tag; }} ondblclick={(e) => e.stopPropagation()}>{tag}</button>
          {/each}
          {#if item.note}<span class="text-dim flex items-center" title={item.note}><Icon name="doc" size={11} /></span>{/if}
          {#if sigView && !sigView.valid}
            <span class="flex items-center gap-1 {sigView.tone === 'bad' ? 'text-bad' : 'text-warn'}" title={t('Programs from known publishers are digitally signed. This one is not, or its signature is broken.')}><Icon name="alert" size={11} />{t(sigView.text)}</span>
          {:else if sigView}
            <span class="flex items-center gap-1 text-ok" title={t('Signed by {name}', { name: sigView.text })}><Icon name="shield" size={11} />{sigView.text}</span>
          {/if}
          {#if item.later && item.status === 'paused'}
            <span class="flex items-center gap-1 text-accent" title={t('Starts when the scheduler runs (Settings → Scheduler).')}><Icon name="clock" size={11} />{t('Waiting for the scheduler')}</span>
          {/if}
          {#if item.quarantined}
            <button type="button" class="flex items-center gap-1 text-warn hover:underline cursor-pointer" title={t('This file is in quarantine. You can restore it in Settings → Multron Cloud.')} onclick={(e) => { e.stopPropagation(); app.settingsSection = 'cloud'; app.view = 'settings'; }} ondblclick={(e) => e.stopPropagation()}><Icon name="lock" size={11} />{t('In quarantine')}</button>
          {/if}
          {#if item.extractState === 'extracting'}
            <span class="flex items-center gap-1 text-accent"><Icon name="archive" size={11} />{t('Extracting…')}</span>
          {:else if item.extractState === 'waiting'}
            <span class="flex items-center gap-1 text-dim" title={t('The archive is extracted when all of its parts have finished.')}><Icon name="archive" size={11} />{t('Waiting for the other parts')}</span>
          {:else if item.extractState === 'done'}
            {@const lines = (item.extractDetail ?? '').split('\n')}
            <button type="button" class="flex items-center gap-1 text-ok hover:underline cursor-pointer" title={lines.slice(1).join('\n')} onclick={(e) => { e.stopPropagation(); if (lines[1]) api.openPath(lines[1]).catch(() => {}); }} ondblclick={(e) => e.stopPropagation()}><Icon name="archive" size={11} />{t('Extracted · {count} files', { count: Number(lines[0]) || 0 })}</button>
          {:else if item.extractState === 'choose'}
            <button type="button" class="flex items-center gap-1 text-warn hover:underline cursor-pointer" title={t('Several archive passwords are saved. Choose which one to use.')} onclick={(e) => { e.stopPropagation(); app.passwordFor = item; }} ondblclick={(e) => e.stopPropagation()}><Icon name="lock" size={11} />{t('Choose a password')}</button>
          {:else if item.extractState === 'password' || item.extractState === 'failed'}
            <button type="button" class="flex items-center gap-1 {item.extractState === 'password' ? 'text-warn' : 'text-bad'} hover:underline cursor-pointer" title={item.extractDetail ?? ''} onclick={(e) => { e.stopPropagation(); if (item.extractState === 'password') { app.settingsSection = 'files'; app.view = 'settings'; } else explain(e, 'error', item.extractDetail || t('No details')); }} ondblclick={(e) => e.stopPropagation()}><Icon name={item.extractState === 'password' ? 'lock' : 'alert'} size={11} />{item.extractState === 'password' ? t('Archive needs a password') : t('Could not extract')}</button>
            <button type="button" class="text-accent hover:underline cursor-pointer" onclick={(e) => { e.stopPropagation(); api.extractDownload(item.id).catch((err) => app.toast('error', String(err))); }} ondblclick={(e) => e.stopPropagation()}>{t('Try again')}</button>
          {/if}
          {#if item.error && item.status === 'failed'}
            <span class="w-[3px] h-[3px] rounded-full bg-dim"></span>
            <span class="text-bad truncate" title={item.error}>{item.error}</span>
            {#if /\b401\b/.test(item.error)}
              <button type="button" class="text-accent hover:underline cursor-pointer whitespace-nowrap flex-shrink-0" onclick={(e) => { e.stopPropagation(); app.loginFor = item; }} ondblclick={(e) => e.stopPropagation()}>{t('Log in…')}</button>
            {/if}
            {#if item.error.includes('CAPTCHA')}
              <button type="button" class="text-accent hover:underline cursor-pointer whitespace-nowrap flex-shrink-0" onclick={(e) => { e.stopPropagation(); api.openUrl(item.url).catch(() => {}); }} ondblclick={(e) => e.stopPropagation()}>{t('Open in browser')}</button>
            {/if}
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

      {#if item.status === 'completed' && app.settings?.cloudScanButton !== false && item.cloudState !== 'scanning'}
        <button
          type="button"
          onclick={scanInCloud}
          ondblclick={(e) => e.stopPropagation()}
          class="chip-btn chip-accent"
          title={t('Scan for malware with Multron Cloud')}
        >
          <Icon name="shield" size={13} />{item.cloudState ? t('Scan again') : t('Scan for malware')}
        </button>
      {/if}
      {#if item.status === 'completed' && !item.quarantined && app.settings?.virustotalButton !== false}
        <button
          type="button"
          onclick={(e) => { e.stopPropagation(); app.virusTotal(item); }}
          ondblclick={(e) => e.stopPropagation()}
          disabled={app.vtBusy !== null}
          class="chip-btn"
          title={t('Check this file on VirusTotal')}
        >
          {#if app.vtBusy === item.id}<Icon name="refresh" size={12} class="animate-spin" />{:else}<Icon name="globe" size={12} />{/if}VirusTotal
        </button>
      {/if}
      <button
        type="button"
        onclick={primary}
        class="round-btn"
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
    <div class="progress-track relative w-full h-[5px] rounded-[3px] overflow-hidden">
      <div
        class="h-full rounded-[3px] transition-[width] duration-500 ease-out {item.status === 'active' ? 'bg-accent progress-active' : item.status === 'failed' ? 'bg-bad/60' : 'bg-dim'} {item.status === 'active' && !barTotal ? 'progress-live w-full' : ''}"
        style={barTotal ? `width: ${pct}%` : ''}
      ></div>
    </div>
  {/if}
</div>
