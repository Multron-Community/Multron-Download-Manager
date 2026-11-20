<script lang="ts">
  import { onMount } from 'svelte';
  import { ask } from '@tauri-apps/plugin-dialog';
  import { app } from '../lib/store.svelte';
  import { api } from '../lib/api';
  import { t } from '../lib/i18n.svelte';
  import { bytes } from '../lib/format';
  import type { HistoryItem } from '../types';
  import Icon from './Icon.svelte';

  let items = $state<HistoryItem[]>([]);
  let search = $state('');
  let loading = $state(true);

  async function load() {
    items = await api.downloadHistory().catch(() => []);
    loading = false;
  }
  onMount(load);

  let shown = $derived.by(() => {
    const q = search.trim().toLowerCase();
    return (q ? items.filter((i) => i.name.toLowerCase().includes(q) || i.url.toLowerCase().includes(q) || i.host.includes(q)) : items).slice(0, 1000);
  });

  async function again(i: HistoryItem) {
    app.openAdd({ url: i.url, dir: i.dir || null, allowDuplicate: true });
  }

  async function remove(i: HistoryItem) {
    await api.downloadHistoryRemove(i.id).catch(() => {});
    items = items.filter((x) => x.id !== i.id);
  }

  async function clearAll() {
    const yes = await ask(t('Clear the whole download history? The downloaded files stay where they are.'), { title: t('Download history'), kind: 'warning', okLabel: t('Clear'), cancelLabel: t('Cancel') }).catch(() => false);
    if (!yes) return;
    await api.downloadHistoryClear().catch(() => {});
    items = [];
  }
</script>

<div class="flex-1 flex flex-col min-w-0 min-h-0 bg-bg">
  <div class="px-5 pt-4 pb-3 border-b border-line flex items-center gap-2 flex-shrink-0">
    <Icon name="clock" size={16} class="text-accent" />
    <span class="title-glow text-[15px] font-semibold mr-2 whitespace-nowrap">{t('Download history')}</span>
    <div class="flex-1 min-w-0 h-[30px] px-[9px] bg-panel border border-line rounded-[8px] flex items-center gap-[7px] focus-within:border-accent">
      <Icon name="search" size={12} class="text-muted flex-shrink-0" />
      <input class="w-full bg-transparent text-[12px] outline-none" placeholder={t('Search by name, site or link…')} bind:value={search} />
    </div>
    <span class="text-[11.5px] text-dim whitespace-nowrap">{t('{count} entries', { count: items.length })}</span>
    <button class="btn !h-[30px]" disabled={!items.length} onclick={clearAll}><Icon name="trash" size={12} />{t('Clear history')}</button>
  </div>
  <div class="flex-1 overflow-y-auto min-h-0">
    {#each shown as i (i.id + i.at)}
      <div class="download-card mx-3 my-1.5 px-4 py-2.5 rounded-[10px] flex items-center gap-3 hover:bg-hover">
        <div class="w-[30px] h-[30px] rounded-[8px] bg-accent/15 text-accent flex items-center justify-center flex-shrink-0"><Icon name={i.kind === 'torrent' ? 'magnet' : i.kind === 'media' ? 'film' : 'file'} size={14} /></div>
        <div class="min-w-0 flex-1">
          <div class="text-[12.5px] truncate" title={i.name}>{i.name}</div>
          <div class="text-[11px] text-dim truncate" title={i.url}>{i.host || i.url}{i.size ? ` · ${bytes(i.size)}` : ''} · {new Date(i.at).toLocaleString()}</div>
        </div>
        <button class="btn !h-[28px]" title={t('Download it again')} onclick={() => again(i)}><Icon name="down" size={12} />{t('Download again')}</button>
        <button class="icon-btn hover:!text-bad" title={t('Remove from history')} onclick={() => remove(i)}><Icon name="x" size={12} /></button>
      </div>
    {:else}
      <div class="h-full flex flex-col items-center justify-center gap-3 text-center px-8">
        <div class="w-[56px] h-[56px] rounded-full bg-accent/15 text-accent flex items-center justify-center"><Icon name={search ? 'search' : 'clock'} size={24} /></div>
        {#if loading}
          <p class="text-[12px] text-muted">…</p>
        {:else if search}
          <div class="text-[14px] font-semibold">{t('Nothing matches the search.')}</div>
        {:else}
          <div class="text-[14px] font-semibold">{t('Download history')}</div>
          <p class="text-[12px] text-muted max-w-[480px]">{t('Finished downloads are remembered here, even after you remove them from the list.')}</p>
        {/if}
      </div>
    {/each}
  </div>
</div>
