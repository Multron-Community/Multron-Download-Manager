<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from '../lib/store.svelte';
  import { api, errorText } from '../lib/api';
  import { t } from '../lib/i18n.svelte';
  import { bytes } from '../lib/format';
  import type { SearchOutcome, SearchResult, SearchSiteView } from '../types';
  import Icon from './Icon.svelte';

  type Sort = 'seeders' | 'size' | 'name' | 'date';

  let sites = $state<SearchSiteView[]>([]);
  let query = $state('');
  let busy = $state(false);
  let outcome = $state<SearchOutcome | null>(null);
  let searched = $state('');
  let sort = $state<Sort>('seeders');
  let filter = $state('');
  let siteFilter = $state('');
  let newName = $state('');
  let newUrl = $state('');
  let addOpen = $state(false);
  let added = $state<string[]>([]);
  let input = $state<HTMLInputElement>();

  let siteName = $derived(Object.fromEntries(sites.map((s) => [s.id, s.name])));
  let enabledCount = $derived(sites.filter((s) => s.enabled).length);
  let validNew = $derived(/^https?:\/\//i.test(newUrl.trim()) && newUrl.includes('{query}'));

  let rows = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    const list = (outcome?.results ?? []).filter(
      (r) => (!siteFilter || r.site === siteFilter) && (!q || r.title.toLowerCase().includes(q))
    );
    const by: Record<Sort, (a: SearchResult, b: SearchResult) => number> = {
      seeders: (a, b) => (b.seeders ?? -1) - (a.seeders ?? -1),
      size: (a, b) => (b.size ?? 0) - (a.size ?? 0),
      name: (a, b) => a.title.localeCompare(b.title),
      date: (a, b) => (b.published ?? '').localeCompare(a.published ?? '')
    };
    return [...list].sort(by[sort]);
  });

  let counts = $derived.by(() => {
    const c: Record<string, number> = {};
    for (const r of outcome?.results ?? []) c[r.site] = (c[r.site] ?? 0) + 1;
    return c;
  });

  async function load() {
    try {
      sites = await api.searchSites();
    } catch (e) {
      app.toast('error', errorText(e));
    }
  }

  async function run() {
    const q = query.trim();
    if (!q || busy) return;
    if (!enabledCount) {
      app.toast('info', t('Turn on at least one site first'));
      return;
    }
    busy = true;
    siteFilter = '';
    filter = '';
    try {
      outcome = await api.torrentSearch(q);
      searched = q;
    } catch (e) {
      app.toast('error', errorText(e));
    } finally {
      busy = false;
    }
  }

  async function toggle(site: SearchSiteView) {
    const s = app.settings;
    if (!s) return;
    const next = site.builtin
      ? {
          ...s,
          searchDisabled: site.enabled ? [...s.searchDisabled, site.id] : s.searchDisabled.filter((d) => d !== site.id)
        }
      : { ...s, searchSites: s.searchSites.map((c) => (c.url === site.id ? { ...c, enabled: !site.enabled } : c)) };
    if (await app.saveSettings(next)) await load();
  }

  async function removeSite(site: SearchSiteView) {
    const s = app.settings;
    if (!s || site.builtin) return;
    if (await app.saveSettings({ ...s, searchSites: s.searchSites.filter((c) => c.url !== site.id) })) await load();
  }

  async function addSite() {
    const s = app.settings;
    const url = newUrl.trim();
    if (!s || !validNew) return;
    if (s.searchSites.some((c) => c.url === url)) {
      app.toast('info', t('This site is already in the list'));
      return;
    }
    if (await app.saveSettings({ ...s, searchSites: [...s.searchSites, { name: newName.trim(), url, enabled: true }] })) {
      newName = '';
      newUrl = '';
      addOpen = false;
      await load();
    }
  }

  function download(r: SearchResult) {
    added = [...added, r.download];
    app.openAdd({ url: r.download, kind: 'torrent', pageTitle: r.title });
  }

  onMount(() => {
    load();
    input?.focus();
  });
</script>

<div class="flex-1 flex min-h-0 min-w-0 overflow-hidden bg-bg">
  <aside class="w-[260px] border-r border-line bg-panel flex flex-col min-h-0">
    <div class="px-4 pt-4 pb-1">
      <span class="text-[14px] font-semibold flex items-center gap-2"><Icon name="search" size={15} />{t('Torrent search')}</span>
      <p class="text-[11px] text-dim mt-1">{t('Sites to search')}</p>
    </div>
    <nav class="flex-1 overflow-y-auto px-2 py-1 space-y-[2px]">
      {#each sites as s (s.id)}
        <div class="site-row group min-h-[40px] px-[10px] py-1 rounded-[9px] flex items-center gap-2.5 text-[12.5px] hover:bg-hover {s.enabled ? 'is-on' : ''}">
          <button
            type="button"
            role="switch"
            aria-checked={s.enabled}
            aria-label={s.name}
            class="relative w-[30px] h-[17px] rounded-full flex-shrink-0 transition-colors {s.enabled ? 'bg-accent' : 'bg-line'}"
            onclick={() => toggle(s)}
          >
            <span class="absolute top-[2px] w-[13px] h-[13px] rounded-full bg-white shadow transition-all {s.enabled ? 'left-[15px]' : 'left-[2px]'}"></span>
          </button>
          <span class="site-avatar w-[24px] h-[24px] rounded-[7px] flex items-center justify-center text-[11px] font-bold flex-shrink-0 uppercase" style="--hue: {[...s.name].reduce((a, c) => a + c.charCodeAt(0), 0) % 360}">{s.name.trim().charAt(0)}</span>
          <span class="flex-1 min-w-0">
            <span class="block truncate {s.enabled ? 'text-white' : 'text-muted'}">{s.name}</span>
            <span class="block text-[10.5px] text-dim truncate" title={s.url}>{s.builtin ? t('Built in') : t('Added by you')}{counts[s.id] !== undefined ? ` · ${counts[s.id]}` : ''}</span>
          </span>
          {#if !s.builtin}
            <button class="icon-btn !w-6 !h-6 opacity-0 group-hover:opacity-100 hover:!text-bad" title={t('Remove')} onclick={() => removeSite(s)}><Icon name="trash" size={12} /></button>
          {/if}
        </div>
      {/each}
    </nav>
    <div class="p-3 border-t border-line space-y-2">
      {#if addOpen}
        <input class="field !h-[30px]" placeholder={t('Name (optional)')} bind:value={newName} />
        <input class="field !h-[30px] mono text-[11px]" placeholder={'https://…/api?t=search&q={query}'} bind:value={newUrl} onkeydown={(e) => e.key === 'Enter' && addSite()} />
        <p class="text-[10.5px] text-dim leading-snug">{t('The site must answer with an RSS, Atom or Torznab feed (for example Jackett or Prowlarr). Put {query} where the search words go.')}</p>
        <div class="flex gap-2">
          <button class="btn btn-accent flex-1" disabled={!validNew} onclick={addSite}><Icon name="plus" size={12} />{t('Add site')}</button>
          <button class="btn" onclick={() => (addOpen = false)}>{t('Cancel')}</button>
        </div>
      {:else}
        <button class="btn w-full" onclick={() => (addOpen = true)}><Icon name="plus" size={12} />{t('Add a site')}</button>
      {/if}
    </div>
  </aside>

  <div class="flex-1 flex flex-col min-w-0 min-h-0">
    <div class="px-5 pt-4 pb-3 border-b border-line flex-shrink-0 space-y-3">
      <form class="flex gap-2" onsubmit={(e) => { e.preventDefault(); run(); }}>
        <div class="flex-1 h-[38px] px-3 bg-panel border border-line rounded-[10px] flex items-center gap-2.5 focus-within:border-accent transition-colors">
          <Icon name="search" size={14} class="text-muted flex-shrink-0" />
          <input bind:this={input} class="w-full bg-transparent text-[13px] outline-none" placeholder={t('Search torrents…')} bind:value={query} />
        </div>
        <button class="btn btn-accent !h-[38px] !px-5" disabled={busy || !query.trim()}>
          {#if busy}<Icon name="refresh" size={13} class="animate-spin" />{/if}{busy ? t('Searching…') : t('Search')}
        </button>
      </form>
      {#if outcome}
        <div class="flex items-center gap-2 flex-wrap">
          <button class="chip {siteFilter === '' ? 'chip-on' : ''}" onclick={() => (siteFilter = '')}>{t('All')} · {outcome.results.length}</button>
          {#each Object.entries(counts) as [id, n] (id)}
            <button class="chip {siteFilter === id ? 'chip-on' : ''}" onclick={() => (siteFilter = siteFilter === id ? '' : id)}>{siteName[id] ?? id} · {n}</button>
          {/each}
          <div class="flex-1"></div>
          <div class="w-[180px] h-[28px] px-[9px] bg-panel border border-line rounded-[8px] flex items-center gap-[7px]">
            <Icon name="filter" size={11} class="text-muted flex-shrink-0" />
            <input class="w-full bg-transparent text-[11.5px] outline-none" placeholder={t('Filter results…')} bind:value={filter} />
          </div>
          <select class="field !h-[28px] !w-auto !text-[11.5px]" bind:value={sort} aria-label={t('Sort by')}>
            <option value="seeders">{t('Most seeders')}</option>
            <option value="size">{t('Largest')}</option>
            <option value="date">{t('Newest')}</option>
            <option value="name">{t('Name')}</option>
          </select>
        </div>
      {/if}
    </div>

    <div class="flex-1 overflow-y-auto min-h-0">
      {#each outcome?.errors ?? [] as err (err.site)}
        <div class="mx-5 mt-3 px-3 py-2 rounded-[8px] bg-bad/10 border border-bad/30 text-[11.5px] flex items-center gap-2">
          <Icon name="alert" size={13} class="text-bad flex-shrink-0" />
          <span class="font-medium">{siteName[err.site] ?? err.site}</span>
          <span class="text-muted truncate" title={err.error}>{err.error}</span>
        </div>
      {/each}
      {#if outcome}
        {#each rows as r (r.site + r.download)}
          {@const done = added.includes(r.download)}
          <div class="download-card mx-3 my-1.5 px-4 py-2.5 rounded-[10px] flex items-center gap-3 hover:bg-hover">
            <div class="w-[30px] h-[30px] rounded-[8px] bg-accent/15 text-accent flex items-center justify-center flex-shrink-0">
              <Icon name={r.download.startsWith('magnet:') ? 'magnet' : 'file'} size={14} />
            </div>
            <div class="min-w-0 flex-1">
              <div class="text-[12.5px] truncate" title={r.title}>{r.title}</div>
              <div class="text-[11px] text-dim truncate">
                {siteName[r.site] ?? r.site}{r.size ? ` · ${bytes(r.size)}` : ''}{r.published ? ` · ${r.published}` : ''}
              </div>
            </div>
            {#if r.seeders != null}
              <div class="w-[78px] flex-shrink-0 text-[11px] mono flex items-center gap-2 justify-end">
                <span class="text-ok flex items-center gap-0.5" title={t('Seeders')}><Icon name="up" size={10} />{r.seeders}</span>
                {#if r.leechers != null}<span class="text-muted flex items-center gap-0.5" title={t('Leechers')}><Icon name="down" size={10} />{r.leechers}</span>{/if}
              </div>
            {/if}
            {#if r.page}
              <button class="icon-btn" title={t('Open page')} onclick={() => api.openUrl(r.page!)}><Icon name="globe" size={13} /></button>
            {/if}
            <button class="btn !h-[28px] {done ? 'text-ok' : ''}" onclick={() => download(r)}>
              <Icon name={done ? 'check' : 'down'} size={12} />{done ? t('Added') : t('Download')}
            </button>
          </div>
        {:else}
          <p class="p-8 text-center text-[12.5px] text-muted">{t('Nothing found for “{query}”.', { query: searched })}</p>
        {/each}
      {:else}
        <div class="h-full flex flex-col items-center justify-center gap-3 text-center px-8">
          <div class="w-[56px] h-[56px] rounded-full bg-accent/15 text-accent flex items-center justify-center"><Icon name="magnet" size={24} /></div>
          <div class="text-[14px] font-semibold">{t('Search torrents on several sites at once')}</div>
          <p class="text-[12px] text-muted max-w-[420px]">{t('Results from every site you turn on appear together. You can add your own sites, such as a Jackett or Prowlarr feed.')}</p>
          <p class="text-[11px] text-dim max-w-[420px]">{t('Only download files you have the right to download.')}</p>
        </div>
      {/if}
    </div>
  </div>
</div>
