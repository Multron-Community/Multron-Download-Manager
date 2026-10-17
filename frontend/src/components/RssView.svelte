<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { app } from '../lib/store.svelte';
  import { api, errorText } from '../lib/api';
  import { t, i18n } from '../lib/i18n.svelte';
  import { bytes, date } from '../lib/format';
  import type { FeedView } from '../types';
  import Icon from './Icon.svelte';

  let feeds = $state<FeedView[]>([]);
  let selected = $state<string | null>(null);
  let busy = $state(false);
  let search = $state('');
  let newFeed = $state('');

  let feed = $derived(feeds.find((f) => f.url === selected) ?? feeds[0] ?? null);
  let items = $derived.by(() => {
    const q = search.trim().toLowerCase();
    return (feed?.state.items ?? []).filter((i) => !q || i.title.toLowerCase().includes(q));
  });

  async function load() {
    try {
      feeds = await api.rssFeeds();
    } catch (e) {
      app.toast('error', errorText(e));
    }
  }

  async function refresh() {
    busy = true;
    try {
      const added = await api.rssRefresh();
      if (added === 0) app.toast('info', 'Feeds are up to date');
    } catch (e) {
      app.toast('error', errorText(e));
    } finally {
      busy = false;
      await load();
    }
  }

  async function addFeed() {
    const url = newFeed.trim();
    if (!app.settings || !/^https?:\/\//i.test(url) || app.settings.rssFeeds.some((f) => f.url === url)) return;
    if (await app.saveSettings({ ...app.settings, rssFeeds: [...app.settings.rssFeeds, { url, name: '', enabled: true }] })) {
      newFeed = '';
      selected = url;
      await refresh();
    }
  }

  async function removeFeed(url: string) {
    if (!app.settings) return;
    await app.saveSettings({ ...app.settings, rssFeeds: app.settings.rssFeeds.filter((f) => f.url !== url) });
    if (selected === url) selected = null;
    await load();
  }

  async function download(itemId: string) {
    if (!feed) return;
    if (await app.run(() => api.rssDownload(feed!.url, itemId), 'Added to downloads')) await load();
  }

  onMount(() => {
    load();
    let off: (() => void) | undefined;
    listen('rss://changed', load).then((fn) => (off = fn));
    return () => off?.();
  });
</script>

<div class="flex-1 flex min-h-0 bg-bg">
  <aside class="w-[260px] border-r border-line bg-panel flex flex-col min-h-0">
    <div class="px-4 pt-4 pb-2 flex items-center justify-between">
      <span class="text-[14px] font-semibold flex items-center gap-2"><Icon name="rss" size={15} />{t('RSS Feeds')}</span>
      <button class="icon-btn" title={t('Check feeds now')} disabled={busy} onclick={refresh}><Icon name="refresh" size={13} class={busy ? 'animate-spin' : ''} /></button>
    </div>
    <nav class="flex-1 overflow-y-auto px-2 space-y-[1px]">
      {#each feeds as f (f.url)}
        <div
          role="button"
          tabindex="0"
          onclick={() => (selected = f.url)}
          onkeydown={(e) => e.key === 'Enter' && (selected = f.url)}
          class="group w-full min-h-[34px] px-[10px] py-1 rounded-[7px] flex items-center gap-2 text-[12.5px] cursor-pointer {feed?.url === f.url ? 'bg-accent-strong/25 text-white' : 'text-muted hover:bg-hover hover:text-white'}"
        >
          <span class="w-[7px] h-[7px] rounded-full flex-shrink-0 {f.state.error ? 'bg-bad' : f.enabled ? 'bg-ok' : 'bg-dim'}"></span>
          <span class="flex-1 min-w-0">
            <span class="block truncate">{f.name}</span>
            <span class="block text-[10.5px] text-dim">{f.state.items.length} · {f.state.updated ? date(f.state.updated, i18n.lang) : t('not checked yet')}</span>
          </span>
          <button class="icon-btn !w-6 !h-6 opacity-0 group-hover:opacity-100 hover:!text-bad" title={t('Remove')} onclick={(e) => { e.stopPropagation(); removeFeed(f.url); }}><Icon name="trash" size={12} /></button>
        </div>
      {:else}
        <p class="px-3 py-2 text-[12px] text-muted">{t('Add a feed below to follow new releases.')}</p>
      {/each}
    </nav>
    <div class="p-3 border-t border-line space-y-2">
      <input class="field !h-[30px]" placeholder="https://…/rss" bind:value={newFeed} onkeydown={(e) => e.key === 'Enter' && addFeed()} />
      <div class="flex gap-2">
        <button class="btn flex-1" onclick={addFeed}><Icon name="plus" size={12} />{t('Add feed')}</button>
        <button class="btn" title={t('Download rules')} onclick={() => { app.view = 'settings'; app.settingsSection = 'rss'; }}><Icon name="settings" size={13} /></button>
      </div>
    </div>
  </aside>

  <div class="flex-1 flex flex-col min-w-0 min-h-0">
    <div class="h-[52px] px-5 border-b border-line flex items-center justify-between gap-3 flex-shrink-0">
      <div class="min-w-0">
        <div class="text-[14px] font-semibold truncate">{feed?.state.title || feed?.name || t('RSS Feeds')}</div>
        {#if feed?.state.error}<div class="text-[11px] text-bad truncate" title={feed.state.error}>{feed.state.error}</div>{/if}
      </div>
      <div class="w-[240px] h-[30px] px-[10px] bg-panel border border-line rounded-[8px] flex items-center gap-[8px]">
        <Icon name="search" size={12} class="text-muted flex-shrink-0" />
        <input class="w-full bg-transparent text-[12px] outline-none" placeholder={t('Search…')} bind:value={search} />
      </div>
    </div>
    <div class="flex-1 overflow-y-auto min-h-0">
      {#each items as item (item.id)}
        {@const done = feed?.downloaded.includes(item.id)}
        <div class="px-5 py-2.5 border-b border-line/60 flex items-center gap-3 hover:bg-hover">
          <Icon name={item.download.startsWith('magnet:') ? 'magnet' : 'file'} size={14} class="text-muted flex-shrink-0" />
          <div class="min-w-0 flex-1">
            <div class="text-[12.5px] truncate" title={item.title}>{item.title}</div>
            <div class="text-[11px] text-dim truncate">
              {item.published ?? ''}{item.size ? ` · ${bytes(item.size)}` : ''}
            </div>
          </div>
          {#if item.page}
            <button class="icon-btn" title={t('Open page')} onclick={() => api.openUrl(item.page!)}><Icon name="globe" size={13} /></button>
          {/if}
          {#if done}
            <span class="text-[11px] text-ok flex items-center gap-1"><Icon name="check" size={12} />{t('Added')}</span>
          {:else}
            <button class="btn !h-[26px]" onclick={() => download(item.id)}><Icon name="down" size={12} />{t('Download')}</button>
          {/if}
        </div>
      {:else}
        <p class="p-6 text-center text-[12px] text-muted">{feed ? t('No items in this feed yet.') : t('No feeds yet.')}</p>
      {/each}
    </div>
  </div>
</div>
