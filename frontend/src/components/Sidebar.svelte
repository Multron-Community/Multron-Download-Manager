<script lang="ts">
  import { app } from '../lib/store.svelte';
  import { t } from '../lib/i18n.svelte';
  import { bytes } from '../lib/format';
  import Icon from './Icon.svelte';
  import type { CategoryFilter, KindFilter, StatusFilter } from '../types';

  const statuses: { id: StatusFilter; label: string; icon: string }[] = [
    { id: 'all', label: 'All Downloads', icon: 'list' },
    { id: 'active', label: 'Active', icon: 'down' },
    { id: 'queued', label: 'Queued', icon: 'clock' },
    { id: 'paused', label: 'Paused', icon: 'pause' },
    { id: 'completed', label: 'Completed', icon: 'check' },
    { id: 'failed', label: 'Failed', icon: 'alert' }
  ];
  const kinds: { id: KindFilter; label: string; icon: string }[] = [
    { id: 'http', label: 'Direct links', icon: 'link' },
    { id: 'media', label: 'Media', icon: 'film' },
    { id: 'torrent', label: 'Torrents', icon: 'magnet' }
  ];
  const categories: { id: CategoryFilter; label: string; icon: string }[] = [
    { id: 'video', label: 'Video', icon: 'film' },
    { id: 'music', label: 'Music', icon: 'music' },
    { id: 'documents', label: 'Documents', icon: 'doc' },
    { id: 'archives', label: 'Archives', icon: 'archive' },
    { id: 'programs', label: 'Programs', icon: 'app' },
    { id: 'images', label: 'Images', icon: 'image' },
    { id: 'other', label: 'Other', icon: 'file' }
  ];

  function pickStatus(id: StatusFilter) {
    app.view = 'downloads';
    app.statusFilter = id;
    app.kindFilter = 'all';
    app.categoryFilter = 'all';
  }
  function pickKind(id: KindFilter) {
    app.view = 'downloads';
    app.kindFilter = app.kindFilter === id ? 'all' : id;
    app.categoryFilter = 'all';
  }
  function pickCategory(id: CategoryFilter) {
    app.view = 'downloads';
    app.categoryFilter = app.categoryFilter === id ? 'all' : id;
    app.kindFilter = 'all';
  }

  let usedPct = $derived(app.disk ? ((app.disk.total - app.disk.free) / app.disk.total) * 100 : 0);
</script>

<aside class="w-[220px] bg-panel border-r border-line flex flex-col p-[14px_12px] flex-shrink-0 h-full">
  <div class="flex items-center gap-2 px-2 pb-4">
    <img src="/logo.png" alt="" class="w-7 h-7 rounded-full" />
    <div class="leading-tight">
      <div class="text-[13px] font-semibold leading-tight">Multron Download Manager</div>
    </div>
  </div>

  <div class="flex-1 overflow-y-auto space-y-5 pr-1">
    <nav class="space-y-[2px]">
      {#each statuses as s}
        {@const on = app.view === 'downloads' && app.statusFilter === s.id && app.kindFilter === 'all' && app.categoryFilter === 'all'}
        <button
          type="button"
          onclick={() => pickStatus(s.id)}
          class="w-full h-[32px] px-[10px] rounded-[8px] flex items-center justify-between text-[12.5px] font-medium transition-colors cursor-pointer {on ? 'bg-line text-white' : 'text-muted hover:bg-hover hover:text-white'}"
        >
          <span class="flex items-center gap-[10px]"><Icon name={s.icon} size={15} />{t(s.label)}</span>
          <span class="text-[11px] {on ? 'text-white' : 'text-dim'}">{app.counts[s.id] ?? 0}</span>
        </button>
      {/each}
    </nav>

    <div class="space-y-[4px]">
      <h3 class="text-[10.5px] font-semibold text-dim px-2 uppercase tracking-wider">{t('Sources')}</h3>
      {#each kinds as k}
        {@const on = app.view === 'downloads' && app.kindFilter === k.id}
        <button
          type="button"
          onclick={() => pickKind(k.id)}
          class="w-full h-[30px] px-[10px] rounded-[8px] flex items-center justify-between text-[12.5px] transition-colors cursor-pointer {on ? 'bg-line text-white' : 'text-muted hover:bg-hover hover:text-white'}"
        >
          <span class="flex items-center gap-[10px]"><Icon name={k.icon} size={15} />{t(k.label)}</span>
          <span class="text-[11px] text-dim">{app.counts[`kind:${k.id}`] ?? 0}</span>
        </button>
      {/each}
    </div>

    <div class="space-y-[4px]">
      <h3 class="text-[10.5px] font-semibold text-dim px-2 uppercase tracking-wider">{t('Categories')}</h3>
      {#each categories as c}
        {@const on = app.view === 'downloads' && app.categoryFilter === c.id}
        <button
          type="button"
          onclick={() => pickCategory(c.id)}
          class="w-full h-[30px] px-[10px] rounded-[8px] flex items-center justify-between text-[12.5px] transition-colors cursor-pointer {on ? 'bg-line text-white' : 'text-muted hover:bg-hover hover:text-white'}"
        >
          <span class="flex items-center gap-[10px]"><Icon name={c.icon} size={15} />{t(c.label)}</span>
          {#if app.counts[`cat:${c.id}`]}
            <span class="text-[11px] text-dim">{app.counts[`cat:${c.id}`]}</span>
          {/if}
        </button>
      {/each}
    </div>
  </div>

  <div class="pt-3 mt-2 border-t border-line/60 space-y-3">
    <div class="flex gap-1 -mb-2">
      <button
        type="button"
        onclick={() => (app.view = app.view === 'rss' ? 'downloads' : 'rss')}
        class="flex-1 h-[32px] px-[10px] rounded-[8px] flex items-center gap-[10px] text-[12.5px] font-medium cursor-pointer {app.view === 'rss' ? 'bg-line text-white' : 'text-muted hover:bg-hover hover:text-white'}"
      >
        <Icon name="rss" size={15} />RSS
      </button>
      <button
        type="button"
        title={t('Create torrent')}
        onclick={() => (app.createTorrentOpen = true)}
        class="h-[32px] px-[10px] rounded-[8px] flex items-center gap-[8px] text-[12.5px] font-medium cursor-pointer text-muted hover:bg-hover hover:text-white"
      >
        <Icon name="magnet" size={14} /><Icon name="plus" size={11} />
      </button>
    </div>
    <button
      type="button"
      onclick={() => (app.extensionSetupOpen = true)}
      class="w-full h-[32px] px-[10px] -mb-2 rounded-[8px] flex items-center justify-between text-[12.5px] font-medium cursor-pointer text-muted hover:bg-hover hover:text-white"
    >
      <span class="flex items-center gap-[10px]"><Icon name="puzzle" size={15} />{t('Browser extension')}</span>
      {#if app.settings?.trustedExtensions.length}
        <span class="w-[7px] h-[7px] rounded-full bg-ok" title={t('Connected')}></span>
      {:else}
        <span class="w-[7px] h-[7px] rounded-full bg-warn" title={t('Not connected')}></span>
      {/if}
    </button>
    <button
      type="button"
      onclick={() => (app.view = app.view === 'settings' ? 'downloads' : 'settings')}
      class="w-full h-[32px] px-[10px] rounded-[8px] flex items-center gap-[10px] text-[12.5px] font-medium cursor-pointer {app.view === 'settings' ? 'bg-line text-white' : 'text-muted hover:bg-hover hover:text-white'}"
    >
      <Icon name="settings" size={15} />{t('Settings')}
    </button>
    {#if app.disk}
      <div class="px-1 space-y-[6px]">
        <div class="flex items-center justify-between text-[11px]">
          <span class="text-muted truncate">{t('Downloads')}</span>
          <span class="text-dim">{bytes(app.disk.free)} {t('free')}</span>
        </div>
        <div class="w-full h-[4px] bg-line rounded-[2px] overflow-hidden">
          <div class="h-full rounded-[2px] {usedPct > 90 ? 'bg-bad' : 'bg-muted'}" style="width: {usedPct}%"></div>
        </div>
      </div>
    {/if}
  </div>
</aside>
