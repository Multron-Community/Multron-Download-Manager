<script lang="ts">
  import { app } from '../lib/store.svelte';
  import { api } from '../lib/api';
  import { t } from '../lib/i18n.svelte';
  import { speed } from '../lib/format';
  import Icon from './Icon.svelte';
  import SpeedGraph from './SpeedGraph.svelte';
  import { resolveTheme } from '../lib/theme';

  let clearMenu = $state(false);

  function clearFailed() {
    clearMenu = false;
    const ids = app.downloads.filter((d) => d.status === 'failed').map((d) => d.id);
    if (ids.length) app.run(() => api.removeMany(ids, false));
  }

  let dark = $derived(resolveTheme(app.settings?.theme ?? 'system') === 'dark');

  const titles: Record<string, string> = {
    all: 'All Downloads',
    active: 'Active',
    queued: 'Queued',
    paused: 'Paused',
    completed: 'Completed',
    failed: 'Failed',
    http: 'Direct links',
    media: 'Media',
    torrent: 'Torrents',
    video: 'Video',
    music: 'Music',
    documents: 'Documents',
    archives: 'Archives',
    programs: 'Programs',
    images: 'Images',
    other: 'Other'
  };

  let title = $derived(
    t(
      titles[
        app.kindFilter !== 'all' ? app.kindFilter : app.categoryFilter !== 'all' ? app.categoryFilter : app.statusFilter
      ]
    )
  );
</script>

<svelte:window onclick={() => (clearMenu = false)} />

<header class="h-[58px] px-5 border-b border-line flex items-center justify-between flex-shrink-0 bg-bg gap-4">
  <div class="flex items-center gap-3 min-w-0">
    <h1 class="text-[15px] font-semibold tracking-tight truncate min-w-[40px]">{title}</h1>
    <div class="h-[24px] px-[8px] bg-line rounded-[6px] flex items-center gap-2 text-[11px] font-medium whitespace-nowrap flex-shrink-0">
      <span class="flex items-center gap-1 text-accent"><Icon name="down" size={12} />{speed(app.stats.downloadSpeed)}</span>
      {#if app.stats.uploadSpeed > 0}
        <span class="flex items-center gap-1 text-seed"><Icon name="up" size={12} />{speed(app.stats.uploadSpeed)}</span>
      {/if}
    </div>
    <div class="hidden xl:block">
      <SpeedGraph values={app.speedHistory} secondary={app.uploadHistory} width={140} height={26} />
    </div>
  </div>

  <div class="flex items-center gap-2 flex-shrink-0">
    <div class="w-[150px] xl:w-[200px] h-[30px] px-[10px] bg-panel border border-line rounded-[8px] flex items-center gap-[8px] focus-within:border-muted/60">
      <Icon name="search" size={12} class="text-muted flex-shrink-0" />
      <input
        type="text"
        placeholder={t('Search…')}
        bind:value={app.search}
        class="w-full bg-transparent text-[12px] text-white placeholder-muted outline-none border-none p-0"
      />
      {#if app.search}
        <button type="button" onclick={() => (app.search = '')} class="text-muted hover:text-white" aria-label={t('Clear')}>
          <Icon name="x" size={12} />
        </button>
      {/if}
    </div>
    <button class="icon-btn" title="Discord" aria-label="Discord" onclick={() => app.run(() => api.openUrl('https://discord.gg/xXmQw3MUAR'))}>
      <svg width="15" height="15" viewBox="-0.5 2 25 25" fill="#5865F2" aria-hidden="true"><path d="M17.5 4.96c-1.33-.6-2.76-1.03-4.26-1.25-.19.34-.4.75-.56 1.12-1.57-.23-3.14-.23-4.69 0-.17-.37-.38-.78-.57-1.12-1.5.22-2.93.65-4.26 1.25C.13 10.63-.4 16.2.22 21.68c1.78 1.3 3.51 2.08 5.23 2.6.43-.58.82-1.19 1.16-1.83-1.68-.62-2.31-1.65-2.31-1.65s.19.12.51.29c.14.07.28.14.41.21.32.14.65.26.98.37 2.1.72 4.31.96 6.55.96 2.23 0 4.45-.24 6.55-.96.33-.11.66-.23.98-.37.14-.07.27-.14.41-.21.32-.17.51-.29.51-.29s-.62 1.03-2.31 1.65c.34.64.73 1.25 1.16 1.83 1.72-.52 3.45-1.3 5.23-2.6.68-6.17-.43-11.64-2.92-16.72zM7.74 17.54c-1.14 0-2.07-1.04-2.07-2.31 0-1.27.92-2.31 2.07-2.31 1.16 0 2.08 1.05 2.07 2.31 0 1.27-.92 2.31-2.07 2.31zm8.52 0c-1.14 0-2.07-1.04-2.07-2.31 0-1.27.92-2.31 2.07-2.31 1.16 0 2.08 1.05 2.07 2.31 0 1.27-.92 2.31-2.07 2.31z" /></svg>
    </button>
    <button class="icon-btn" title={t('Support Multron Download Manager on Patreon')} aria-label={t('Support Multron Download Manager on Patreon')} onclick={() => app.run(() => api.openUrl('https://www.patreon.com/cw/multron'))}>
      <svg width="14" height="14" viewBox="0 0 24 24" fill="#FF424D" aria-hidden="true"><path d="M15.386.524c-4.764 0-8.64 3.876-8.64 8.64 0 4.75 3.876 8.613 8.64 8.613 4.75 0 8.614-3.864 8.614-8.613C24 4.4 20.136.524 15.386.524M.003 23.537h4.22V.524H.003" /></svg>
    </button>
    <button class="icon-btn" title={dark ? t('Switch to light theme') : t('Switch to dark theme')} onclick={() => app.toggleTheme()}>
      <Icon name={dark ? 'sun' : 'moon'} size={14} />
    </button>
    <button
      type="button"
      class="h-[28px] px-[10px] rounded-full text-[11.5px] font-medium flex items-center gap-1.5 flex-shrink-0 whitespace-nowrap cursor-pointer transition-colors {app.settings?.sequential ? 'bg-accent-strong/25 text-accent' : 'text-muted hover:bg-line hover:text-white'}"
      title={t('Sequential download: one at a time, in queue order. Turn off to download in parallel.')}
      onclick={() => app.toggleSequential()}
    >
      <Icon name="queue" size={14} /><span class="hidden xl:inline">{app.settings?.sequential ? t('One at a time') : t('Parallel')}</span>
    </button>
    {#if app.settings?.cloudScanAllButton !== false}
      <button
        class="h-[28px] px-[10px] rounded-full text-[11.5px] font-medium flex items-center gap-1.5 flex-shrink-0 whitespace-nowrap cursor-pointer transition-colors {app.cloudReports.all?.running ? 'bg-accent-strong/30 text-accent' : 'bg-accent/10 text-accent hover:bg-accent/20'}"
        title={t('Scan every finished download with Multron Cloud')}
        aria-label={t('Scan all')}
        onclick={() => app.scanAllInCloud()}
      >
        <Icon name="shield" size={14} /><span class="hidden lg:inline">{app.cloudReports.all?.running ? t('Scanning {done}/{total}', { done: app.cloudReports.all.done, total: app.cloudReports.all.total }) : t('Scan all')}</span>
      </button>
    {/if}
    <button class="icon-btn" title={t('Pause all')} onclick={() => app.run(api.pauseAll)}><Icon name="pause" size={14} /></button>
    <button class="icon-btn" title={t('Resume all')} onclick={() => app.run(api.resumeAll)}><Icon name="play" size={14} /></button>
    <div class="relative">
      <button class="icon-btn" title={t('Clear list')} onclick={(e) => { e.stopPropagation(); clearMenu = !clearMenu; }}><Icon name="check" size={14} /></button>
      {#if clearMenu}
        <div class="absolute right-0 top-[34px] z-40 min-w-[200px] p-1 rounded-[10px] bg-menu border border-line shadow-2xl text-[12.5px]">
          <button class="w-full text-left px-3 py-2 rounded-[7px] hover:bg-line" onclick={() => { clearMenu = false; app.run(api.clearFinished); }}>{t('Clear finished')}</button>
          <button class="w-full text-left px-3 py-2 rounded-[7px] hover:bg-line" onclick={clearFailed}>{t('Clear failed')}</button>
          <div class="h-px bg-line my-1"></div>
          <button class="w-full text-left px-3 py-2 rounded-[7px] hover:bg-line text-bad" onclick={() => { clearMenu = false; app.askRemove([...app.downloads]); }}>{t('Clear all…')}</button>
        </div>
      {/if}
    </div>
    <button type="button" onclick={() => app.openAdd()} class="btn btn-primary h-[30px]">
      <Icon name="plus" size={12} stroke={3} /><span>{t('New Task')}</span>
    </button>
  </div>
</header>
