<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { app } from '../lib/store.svelte';
  import { api, errorText } from '../lib/api';
  import { t, i18n } from '../lib/i18n.svelte';
  import { bytes, date } from '../lib/format';
  import type { AddRequest, FeedView } from '../types';
  import Icon from './Icon.svelte';

  let feeds = $state<FeedView[]>([]);
  let selected = $state<string | null>(null);
  let busy = $state(false);
  let search = $state('');
  let newFeed = $state('');
  let gallery = $state(false);
  let ytLink = $state('');
  let iaTerm = $state('');
  let iaType = $state('');
  const IA_TYPES = [
    ['', 'Everything'],
    ['movies', 'Video'],
    ['audio', 'Music'],
    ['software', 'Programs'],
    ['texts', 'Books and texts'],
    ['image', 'Images'],
    ['data', 'Data']
  ];

  function archiveFeed(term: string, type: string) {
    const parts = [term.trim() || '*'];
    if (type) parts.push(`mediatype:${type}`);
    const q = parts.join(' AND ');
    return `https://archive.org/advancedsearch.php?q=${encodeURIComponent(q)}&fl%5B%5D=identifier&sort%5B%5D=addeddate+desc&rows=100&output=rss`;
  }

  async function addArchive() {
    const label = [iaTerm.trim(), iaType ? t(IA_TYPES.find((x) => x[0] === iaType)![1]) : ''].filter(Boolean).join(' · ') || t('Everything');
    if (await addUrl(archiveFeed(iaTerm, iaType), `Internet Archive: ${label}`)) iaTerm = '';
  }
  let query = $state('');
  let finding = $state(false);
  type Found = { url: string; title: string; sub: string; artwork: string | null };
  let found = $state<Found[] | null>(null);
  let foundFor = $state('');

  const BUILT_IN = [
    { url: 'https://fosstorrents.com/feed/torrents.xml#only=Ubuntu%20', name: 'Ubuntu', desc: 'Official torrents of Ubuntu Desktop and Server releases, through FOSS Torrents.', icon: 'app', tags: 'linux ubuntu canonical' },
    { url: 'https://fosstorrents.com/feed/torrents.xml#only=Linux%20Mint', name: 'Linux Mint', desc: 'Official torrents of every Linux Mint edition, through FOSS Torrents.', icon: 'app', tags: 'linux mint cinnamon' },
    { url: 'https://torrent.fedoraproject.org/rss20.xml', name: 'Fedora Linux', desc: 'Official torrents of every Fedora edition, from the Fedora Project.', icon: 'app', tags: 'linux fedora red hat' },
    { url: 'https://archlinux.org/feeds/releases/', name: 'Arch Linux', desc: 'The monthly Arch Linux installer, from archlinux.org.', icon: 'app', tags: 'linux arch' },
    { url: 'https://distrowatch.com/news/torrents.xml', name: 'DistroWatch', desc: 'New torrents of many Linux distributions, collected by DistroWatch.', icon: 'magnet', tags: 'linux distributions debian rocky alma' },
    { url: 'https://fosstorrents.com/feed/torrents.xml', name: 'FOSS Torrents', desc: 'Free and open-source programs, games and Linux systems, shared as torrents.', icon: 'cube', tags: 'open source software games linux' },
    { url: 'https://academictorrents.com/rss.xml', name: 'Academic Torrents', desc: 'Research data sets, papers and course material shared by universities.', icon: 'book', tags: 'science research data papers' },
    { url: 'https://archive.org/services/collection-rss.php?collection=opensource_movies', name: 'Internet Archive: Community video', desc: 'Films and videos shared freely on the Internet Archive.', icon: 'film', tags: 'video film movies archive' },
    { url: 'https://archive.org/services/collection-rss.php?collection=opensource_audio', name: 'Internet Archive: Community audio', desc: 'Music and recordings shared freely on the Internet Archive.', icon: 'music', tags: 'music audio archive' }
  ];
  let subscribed = $derived(new Set((app.settings?.rssFeeds ?? []).map((f) => f.url)));
  let showGallery = $derived(gallery || !feeds.length);
  let builtIn = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q || looksLikeSite(q)) return BUILT_IN;
    return BUILT_IN.filter((b) => (b.name + ' ' + t(b.name) + ' ' + b.tags + ' ' + t(b.desc)).toLowerCase().includes(q));
  });

  let feed = $derived(feeds.find((f) => f.url === selected) ?? feeds[0] ?? null);
  let items = $derived.by(() => {
    const q = search.trim().toLowerCase();
    return (feed?.state.items ?? []).filter((i) => !q || i.title.toLowerCase().includes(q));
  });

  function looksLikeSite(text: string) {
    return /^https?:\/\//i.test(text) || (/^[\w-]+(\.[\w-]+)+(\/\S*)?$/.test(text) && !/\s/.test(text));
  }

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

  async function addUrl(input: string, name = '') {
    if (!app.settings || !/^https?:\/\//i.test(input)) return false;
    let url = input;
    try {
      url = (await api.rssResolve(input)).url;
    } catch (e) {
      app.toast('error', errorText(e));
      return false;
    }
    if (app.settings.rssFeeds.some((f) => f.url === url)) {
      selected = url;
      gallery = false;
      return false;
    }
    if (!(await app.saveSettings({ ...app.settings, rssFeeds: [...app.settings.rssFeeds, { url, name, enabled: true }] }))) return false;
    selected = url;
    gallery = false;
    await refresh();
    return true;
  }

  async function addFeed() {
    if (await addUrl(newFeed.trim())) newFeed = '';
  }

  async function addYoutube() {
    if (await addUrl(ytLink.trim())) ytLink = '';
  }

  async function find() {
    const q = query.trim();
    if (!q || finding) return;
    finding = true;
    try {
      if (looksLikeSite(q)) {
        found = (await api.discoverFeeds(q)).map((f) => ({ url: f.url, title: f.title || f.url, sub: f.title ? f.url : '', artwork: null }));
      } else {
        found = (await api.podcastSearch(q)).map((p) => ({ url: p.feedUrl, title: p.name, sub: p.author + (p.episodes ? ` · ${t('{count} episodes', { count: p.episodes })}` : ''), artwork: p.artwork }));
      }
      foundFor = q;
    } catch (e) {
      app.toast('error', errorText(e));
    } finally {
      finding = false;
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

  function chooseFiles(item: { id: string; download: string; title: string }) {
    const url = feed?.url;
    if (!url) return;
    app.openAdd({ url: item.download, pageTitle: item.title } as AddRequest, null, () => {
      api.rssMark(url, item.id).then(load).catch(() => {});
    });
  }

  onMount(() => {
    load();
    let off: (() => void) | undefined;
    listen('rss://changed', load).then((fn) => (off = fn));
    return () => off?.();
  });
</script>

<div class="flex-1 flex min-h-0 min-w-0 overflow-hidden bg-bg">
  <aside class="w-[260px] border-r border-line bg-panel flex flex-col min-h-0">
    <div class="px-4 pt-4 pb-2 flex items-center justify-between">
      <span class="text-[14px] font-semibold flex items-center gap-2"><Icon name="rss" size={15} />{t('RSS Feeds')}</span>
      <button class="icon-btn" title={t('Check feeds now')} disabled={busy} onclick={refresh}><Icon name="refresh" size={13} class={busy ? 'animate-spin' : ''} /></button>
    </div>
    <div class="px-2 pb-2">
      <button type="button" class="w-full h-[32px] px-[10px] rounded-[7px] flex items-center gap-2 text-[12.5px] cursor-pointer {showGallery ? 'bg-accent-strong/25 text-white' : 'text-muted hover:bg-hover hover:text-white'}" onclick={() => (gallery = true)}>
        <Icon name="search" size={13} />{t('Find feeds')}
      </button>
    </div>
    <nav class="flex-1 overflow-y-auto px-2 space-y-[1px]">
      {#each feeds as f (f.url)}
        <div
          role="button"
          tabindex="0"
          onclick={() => { selected = f.url; gallery = false; }}
          onkeydown={(e) => e.key === 'Enter' && ((selected = f.url), (gallery = false))}
          class="group w-full min-h-[34px] px-[10px] py-1 rounded-[7px] flex items-center gap-2 text-[12.5px] cursor-pointer {!showGallery && feed?.url === f.url ? 'bg-accent-strong/25 text-white' : 'text-muted hover:bg-hover hover:text-white'}"
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

  {#if showGallery}
    <div class="flex-1 flex flex-col min-w-0 min-h-0 overflow-y-auto">
      <div class="px-6 pt-5 pb-4 space-y-3">
        <div class="flex items-start justify-between gap-4">
          <div>
            <div class="text-[15px] font-semibold flex items-center gap-2"><Icon name="search" size={16} class="text-accent" />{t('Find feeds')}</div>
            <p class="text-[12px] text-muted mt-1 max-w-[620px]">{t('Search podcasts by name, or paste a website address to find its feeds. Below are built-in feeds from official sources that share their files legally.')}</p>
          </div>
          {#if feeds.length}<button class="icon-btn" aria-label={t('Close')} onclick={() => (gallery = false)}><Icon name="x" size={14} /></button>{/if}
        </div>
        <div class="flex gap-2 max-w-[720px]">
          <div class="flex-1 min-w-0 h-[36px] px-3 bg-panel border border-line rounded-[10px] flex items-center gap-2 focus-within:border-accent">
            <Icon name="search" size={14} class="text-muted flex-shrink-0" />
            <input class="w-full bg-transparent text-[13px] outline-none" placeholder={t('Podcast name, topic or website address…')} bind:value={query} onkeydown={(e) => e.key === 'Enter' && find()} />
          </div>
          <button class="btn btn-accent !h-[36px] !px-4" disabled={!query.trim() || finding} onclick={find}>
            <Icon name={finding ? 'refresh' : 'search'} size={13} class={finding ? 'animate-spin' : ''} />{looksLikeSite(query.trim()) ? t('Find feeds') : t('Search podcasts')}
          </button>
        </div>
        {#if found}
          <div class="max-w-[720px] download-card rounded-[12px] border border-line">
            <div class="px-4 py-2 border-b border-line/60 flex items-center justify-between text-[11.5px] text-dim">
              <span>{t('{count} results for “{query}”', { count: found.length, query: foundFor })}</span>
              <button class="icon-btn !w-6 !h-6" aria-label={t('Close')} onclick={() => (found = null)}><Icon name="x" size={11} /></button>
            </div>
            <div class="max-h-[320px] overflow-y-auto p-1">
              {#each found as f (f.url)}
                {@const added = subscribed.has(f.url)}
                <div class="flex items-center gap-3 px-3 py-2 rounded-[8px] hover:bg-hover">
                  {#if f.artwork}<img src={f.artwork} alt="" class="w-[38px] h-[38px] rounded-[8px] object-cover flex-shrink-0" referrerpolicy="no-referrer" />{:else}<div class="w-[38px] h-[38px] rounded-[8px] bg-accent/15 text-accent flex items-center justify-center flex-shrink-0"><Icon name="rss" size={15} /></div>{/if}
                  <div class="min-w-0 flex-1">
                    <div class="text-[12.5px] truncate" title={f.title}>{f.title}</div>
                    {#if f.sub}<div class="text-[11px] text-dim truncate" title={f.sub}>{f.sub}</div>{/if}
                  </div>
                  {#if added}
                    <span class="text-[11px] text-ok flex items-center gap-1"><Icon name="check" size={12} />{t('Added')}</span>
                  {:else}
                    <button class="btn !h-[28px]" disabled={busy} onclick={() => addUrl(f.url, f.title !== f.url ? f.title : '')}><Icon name="plus" size={12} />{t('Add feed')}</button>
                  {/if}
                </div>
              {:else}
                <p class="px-3 py-4 text-[12px] text-muted">{t('No feeds found.')}</p>
              {/each}
            </div>
          </div>
        {/if}
      </div>

      <div class="px-6 pb-2 text-[10.5px] font-semibold text-dim uppercase tracking-wider flex items-center gap-1.5"><Icon name="shield" size={11} />{t('Built-in feeds')}</div>
      <div class="px-6 pb-5 grid grid-cols-[repeat(auto-fill,minmax(250px,1fr))] gap-3">
        {#each builtIn as b (b.url)}
          {@const added = subscribed.has(b.url)}
          <div class="download-card rounded-[12px] border border-line p-4 flex flex-col gap-2">
            <div class="flex items-center gap-2.5">
              <div class="site-avatar w-[32px] h-[32px] rounded-[9px] flex items-center justify-center flex-shrink-0" style="--hue: {[...b.name].reduce((a, c) => a * 7 + c.charCodeAt(0), 0) % 360}"><Icon name={b.icon} size={15} /></div>
              <div class="min-w-0">
                <div class="text-[13px] font-semibold truncate">{t(b.name)}</div>
                <div class="text-[10.5px] text-dim truncate">{new URL(b.url).hostname}</div>
              </div>
            </div>
            <p class="text-[12px] text-muted leading-snug flex-1">{t(b.desc)}</p>
            {#if added}
              <button class="btn !h-[28px]" onclick={() => { selected = b.url; gallery = false; }}><Icon name="check" size={12} class="text-ok" />{t('Added')}</button>
            {:else}
              <button class="btn btn-accent !h-[28px]" disabled={busy} onclick={() => addUrl(b.url, t(b.name))}><Icon name="plus" size={12} />{t('Add feed')}</button>
            {/if}
          </div>
        {:else}
          <p class="text-[12px] text-muted">{t('No built-in feed matches the search.')}</p>
        {/each}
      </div>

      <div class="px-6 pb-4 max-w-[760px]">
        <section class="download-card rounded-[12px] border border-line p-4 space-y-2.5">
          <div class="text-[13px] font-semibold flex items-center gap-2"><Icon name="film" size={14} class="text-accent" />{t('YouTube and YouTube Music')}</div>
          <p class="text-[12px] text-muted leading-snug">{t('Paste the link of a channel, an artist or a playlist. New videos and songs appear here, and the download uses the video quality from Settings → Media.')}</p>
          <div class="flex gap-2">
            <input class="field !h-[30px] flex-1 min-w-0" placeholder="https://www.youtube.com/@…" bind:value={ytLink} onkeydown={(e) => e.key === 'Enter' && addYoutube()} />
            <button class="btn btn-accent !h-[30px]" disabled={!ytLink.trim() || busy} onclick={addYoutube}><Icon name="plus" size={12} />{t('Add feed')}</button>
          </div>
        </section>
      </div>
      <div class="px-6 pb-4 max-w-[760px]">
        <section class="download-card rounded-[12px] border border-line p-4 space-y-2.5">
          <div class="text-[13px] font-semibold flex items-center gap-2"><Icon name="archive" size={14} class="text-accent" />Internet Archive</div>
          <p class="text-[12px] text-muted leading-snug">{t('Follow anything on archive.org: type a search, or leave it empty to see everything new. Each item is downloaded completely through its own torrent. Some uploads there are not licensed for sharing, so check the item page first.')}</p>
          <div class="flex gap-2">
            <input class="field !h-[30px] flex-1 min-w-0" placeholder={t('Search archive.org, for example linux or collection:prelinger')} bind:value={iaTerm} onkeydown={(e) => e.key === 'Enter' && addArchive()} />
            <select class="field !h-[30px] !w-auto text-[12px]" bind:value={iaType}>
              {#each IA_TYPES as [id, label] (id)}<option value={id}>{t(label)}</option>{/each}
            </select>
            <button class="btn btn-accent !h-[30px]" disabled={busy} onclick={addArchive}><Icon name="plus" size={12} />{t('Add feed')}</button>
          </div>
        </section>
      </div>
      <p class="px-6 pb-6 text-[11.5px] text-dim max-w-[760px]">{t('Spotify has no feeds, and its music is copy-protected, so it cannot be downloaded. Podcasts on Spotify usually have their own public feed: find them with Search podcasts.')}</p>
    </div>
  {:else}
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
            <Icon name={item.download.startsWith('magnet:') ? 'magnet' : /youtube\.com|youtu\.be/.test(item.download) ? 'film' : 'file'} size={14} class="text-muted flex-shrink-0" />
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
              {#if !/youtube\.com|youtu\.be/.test(item.download)}
                <button class="btn !h-[26px]" title={t('Choose files')} onclick={() => chooseFiles(item)}><Icon name="list" size={12} />{t('Choose files')}</button>
              {/if}
              <button class="btn !h-[26px]" onclick={() => download(item.id)}><Icon name="down" size={12} />{t('Download')}</button>
            {/if}
          </div>
        {:else}
          <p class="p-6 text-center text-[12px] text-muted">{feed ? t('No items in this feed yet.') : t('No feeds yet.')}</p>
        {/each}
      </div>
    </div>
  {/if}
</div>
