<script lang="ts">
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { app } from '../lib/store.svelte';
  import { t } from '../lib/i18n.svelte';
  import { bytes } from '../lib/format';
  import { linksFromText } from '../lib/links';
  import { grabber, fileType, extOf, hostOf, DEFAULT_CHECK_TYPES, SPEED_DEFAULTS, type FileType, type GrabLink, type LinkOptions, type Priority } from '../lib/grabber.svelte';
  import Icon from './Icon.svelte';

  const TYPE_LABELS: Record<FileType, { label: string; icon: string }> = {
    video: { label: 'Video', icon: 'film' },
    audio: { label: 'Music', icon: 'music' },
    archive: { label: 'Archives', icon: 'archive' },
    document: { label: 'Documents', icon: 'doc' },
    image: { label: 'Images', icon: 'image' },
    program: { label: 'Programs', icon: 'app' },
    page: { label: 'Web pages', icon: 'globe' },
    web: { label: 'Scripts & styles', icon: 'code' },
    other: { label: 'Other', icon: 'file' }
  };

  let search = $state('');
  let hiddenHosts = $state<Set<string>>(new Set());
  let hiddenExts = $state<Set<string>>(new Set());
  let collapsed = $state<Set<string>>(new Set());
  let selected = $state<Set<number>>(new Set());
  let dir = $state(app.settings?.downloadDir ?? '');
  let paused = $state(false);
  let busy = $state(false);

  let pasteText = $state('');
  let addDir = $state(app.settings?.downloadDir ?? '');
  let pkgName = $state('');
  let note = $state('');
  let extractPassword = $state('');
  let autoExtract = $state(app.settings?.autoExtract ?? true);
  let loginUser = $state('');
  let loginPassword = $state('');
  let priority = $state<Priority>('normal');
  let analysing = $state(false);

  let prefs = $derived(grabber.prefs);
  let hiddenTypes = $derived(new Set(prefs.hiddenTypes));
  let pasted = $derived(linksFromText(pasteText));
  let all = $derived(grabber.links);
  let typed = $derived(all.map((l) => ({ l, type: fileType(l.name, l.url), ext: extOf(l.name) })));
  let visible = $derived.by(() => {
    const q = search.trim().toLowerCase();
    return typed
      .filter(
        ({ l, type, ext }) =>
          (prefs.showOffline || l.status !== 'offline') &&
          !hiddenTypes.has(type) &&
          !hiddenExts.has(ext) &&
          !hiddenHosts.has(l.host) &&
          (!prefs.sameSite || !l.page || hostOf(l.page) === l.host) &&
          (!q || l.name.toLowerCase().includes(q) || l.url.toLowerCase().includes(q) || l.pkg.toLowerCase().includes(q) || (l.text ?? '').toLowerCase().includes(q))
      )
      .map((x) => x.l);
  });
  let packages = $derived.by(() => {
    const map = new Map<string, GrabLink[]>();
    for (const l of visible) {
      const list = map.get(l.pkg);
      if (list) list.push(l);
      else map.set(l.pkg, [l]);
    }
    return [...map.entries()].map(([name, links]) => ({
      name,
      links,
      size: links.reduce((a, l) => a + (l.size ?? 0), 0),
      unknownSize: links.some((l) => l.size == null),
      online: links.filter((l) => l.status === 'online').length,
      offline: links.filter((l) => l.status === 'offline').length,
      checking: links.some((l) => l.status === 'checking'),
      hosts: [...new Set(links.map((l) => l.host))]
    }));
  });
  function counts<K extends string>(key: (x: (typeof typed)[number]) => K) {
    const c = new Map<K, number>();
    for (const x of typed) c.set(key(x), (c.get(key(x)) ?? 0) + 1);
    return [...c.entries()].sort((a, b) => b[1] - a[1]);
  }
  let typeCounts = $derived(new Map(counts((x) => x.type)));
  let extCounts = $derived(counts((x) => x.ext).filter(([e]) => e));
  let hostCounts = $derived(counts((x) => x.l.host));
  let stats = $derived({
    packages: new Set(all.map((l) => l.pkg)).size,
    links: all.length,
    shown: visible.length,
    bytes: all.reduce((a, l) => a + (l.size ?? 0), 0),
    hosts: new Set(all.map((l) => l.host)).size,
    online: all.filter((l) => l.status === 'online').length,
    offline: all.filter((l) => l.status === 'offline').length,
    unknown: all.filter((l) => l.status === 'unknown').length,
    checking: all.filter((l) => l.status === 'checking').length
  });
  let activeFilters = $derived(hiddenTypes.size + hiddenExts.size + hiddenHosts.size + (prefs.showOffline ? 0 : 1) + (prefs.sameSite ? 1 : 0));
  let chosen = $derived(visible.filter((l) => selected.has(l.id)));
  let startable = $derived((chosen.length ? chosen : visible).filter((l) => l.status !== 'offline' && l.status !== 'checking'));

  function flip<T>(set: Set<T>, value: T) {
    const next = new Set(set);
    if (next.has(value)) next.delete(value);
    else next.add(value);
    return next;
  }

  function setPref<K extends keyof typeof grabber.prefs>(key: K, value: (typeof grabber.prefs)[K]) {
    grabber.prefs = { ...grabber.prefs, [key]: value };
    grabber.savePrefs();
  }

  function toggleType(type: FileType) {
    setPref('hiddenTypes', [...flip(hiddenTypes, type)]);
  }

  let speedChanged = $derived(
    prefs.parallel !== SPEED_DEFAULTS.parallel ||
      prefs.pageTimeout !== SPEED_DEFAULTS.pageTimeout ||
      prefs.subpageLimit !== SPEED_DEFAULTS.subpageLimit ||
      prefs.checkTypes.length !== DEFAULT_CHECK_TYPES.length ||
      DEFAULT_CHECK_TYPES.some((x) => !prefs.checkTypes.includes(x))
  );

  function toggleCheckType(type: FileType) {
    setPref('checkTypes', prefs.checkTypes.includes(type) ? prefs.checkTypes.filter((x) => x !== type) : [...prefs.checkTypes, type]);
  }

  function setNum(key: 'parallel' | 'pageTimeout' | 'subpageLimit', value: number, lo: number, hi: number) {
    if (!Number.isFinite(value)) return;
    setPref(key, Math.min(hi, Math.max(lo, Math.round(value))));
  }

  function resetSpeed() {
    grabber.prefs = { ...grabber.prefs, checkTypes: [...DEFAULT_CHECK_TYPES], ...SPEED_DEFAULTS };
    grabber.savePrefs();
  }

  function resetFilters() {
    hiddenExts = new Set();
    hiddenHosts = new Set();
    grabber.prefs = { ...grabber.prefs, hiddenTypes: [], showOffline: true, sameSite: false };
    grabber.savePrefs();
  }

  function selectPackage(links: GrabLink[], on: boolean) {
    const next = new Set(selected);
    for (const l of links) on ? next.add(l.id) : next.delete(l.id);
    selected = next;
  }

  function openAdd() {
    addDir = dir || app.settings?.downloadDir || '';
    autoExtract = app.settings?.autoExtract ?? true;
    grabber.addOpen = true;
  }

  function closeAdd() {
    grabber.addOpen = false;
    pasteText = '';
    pkgName = '';
    note = '';
    extractPassword = '';
    loginUser = '';
    loginPassword = '';
    priority = 'normal';
  }

  async function analyse() {
    if (!pasted.length || analysing) return;
    analysing = true;
    const opts: LinkOptions = { note: note.trim(), extractPassword: extractPassword.trim(), autoExtract, loginUser: loginUser.trim(), loginPassword, priority };
    const urls = pasted;
    const pkg = pkgName.trim();
    if (addDir) dir = addDir;
    closeAdd();
    const n = await grabber.analyse(urls, opts, pkg);
    analysing = false;
    app.toast(n ? 'success' : 'info', n ? t('{count} links found', { count: n }) : t('No new links were found.'));
  }

  async function browse(set: (v: string) => void, current: string) {
    const res = await openDialog({ directory: true, defaultPath: current || undefined });
    if (typeof res === 'string') set(res);
  }

  async function startLinks() {
    if (!startable.length || busy) return;
    busy = true;
    const n = await grabber.start(startable.map((l) => l.id), dir || null, paused);
    busy = false;
    selected = new Set();
    if (n) app.toast('success', t('{count} downloads added', { count: n }));
  }

  function removeChosen() {
    grabber.remove((chosen.length ? chosen : []).map((l) => l.id));
    selected = new Set();
  }

  const statusTone: Record<string, string> = { online: 'text-ok', offline: 'text-bad', unknown: 'text-warn', checking: 'text-muted' };
  const statusIcon: Record<string, string> = { online: 'check', offline: 'x', unknown: 'info', checking: 'refresh' };
  const tagLabel: Record<string, string> = { a: 'Link', img: 'Image', video: 'Video', audio: 'Audio', source: 'Media', iframe: 'Frame', script: 'Script', link: 'Style', meta: 'Page info', text: 'Text' };
  const cols = 'grid-cols-[28px_minmax(0,1fr)_92px_140px_84px_112px]';
</script>

{#snippet chip(on: boolean, label: string, count: number, icon: string | null, toggle: () => void)}
  <button type="button" class="h-[26px] px-2.5 rounded-full border text-[11.5px] flex items-center gap-1.5 cursor-pointer transition-colors max-w-[220px] {on ? 'border-accent/50 bg-accent/12 text-white' : 'border-line text-dim line-through decoration-dim/60 hover:text-muted'}" onclick={toggle}>
    {#if icon}<Icon name={icon} size={11} class="flex-shrink-0" />{/if}<span class="truncate">{label}</span><span class="text-[10.5px] opacity-70">{count}</span>
  </button>
{/snippet}

{#snippet switchRow(on: boolean, label: string, hint: string, toggle: () => void)}
  <label class="flex items-start gap-2 text-[12px] cursor-pointer py-1">
    <input type="checkbox" class="check mt-[2px]" checked={on} onchange={toggle} />
    <span><span class="text-white/90">{label}</span>{#if hint}<span class="block text-[11px] text-dim leading-snug">{hint}</span>{/if}</span>
  </label>
{/snippet}

<div class="flex-1 flex flex-col min-h-0 min-w-0 overflow-hidden bg-bg">
  <div class="px-4 pt-3 pb-2.5 border-b border-line flex items-center gap-2 flex-shrink-0">
    <span class="text-[14px] font-semibold flex items-center gap-2 mr-2 whitespace-nowrap"><Icon name="link" size={15} />LinkGrabber</span>
    <div class="flex-1 min-w-0 h-[30px] px-[9px] bg-panel border border-line rounded-[8px] flex items-center gap-[7px] focus-within:border-accent">
      <Icon name="search" size={12} class="text-muted flex-shrink-0" />
      <input class="w-full bg-transparent text-[12px] outline-none" placeholder={t('Search by file name…')} bind:value={search} />
    </div>
    <button class="btn btn-accent !h-[30px]" onclick={openAdd}><Icon name="plus" size={12} />{t('Add new links')}</button>
    <button class="icon-btn" title={t('Check again')} onclick={() => grabber.recheck(chosen.length ? chosen.map((l) => l.id) : undefined)}><Icon name="refresh" size={13} /></button>
    <button class="icon-btn" title={t('Remove offline links')} disabled={!stats.offline} onclick={() => grabber.remove(all.filter((l) => l.status === 'offline').map((l) => l.id))}><Icon name="alert" size={13} /></button>
    <button class="icon-btn hover:!text-bad" title={chosen.length ? t('Remove selected') : t('Clear the list')} disabled={!all.length} onclick={() => (chosen.length ? removeChosen() : grabber.remove(all.map((l) => l.id)))}><Icon name="trash" size={13} /></button>
  </div>

  {#if grabber.busy}
    {@const pct = grabber.checkTotal ? Math.min(100, (grabber.checkDone / grabber.checkTotal) * 100) : 0}
    <div class="px-4 py-2 border-b border-line bg-accent/5 flex-shrink-0 space-y-1.5">
      <div class="flex items-center gap-2 text-[11.5px]">
        <Icon name="refresh" size={11} class="animate-spin text-accent flex-shrink-0" />
        <span class="text-white/90">
          {#if grabber.scanning}{t('Scanning pages for links… ({count} left)', { count: grabber.scanning })}{#if grabber.scanned} · {t('{count} pages done', { count: grabber.scanned })}{/if}{/if}
          {#if grabber.scanning && grabber.checkTotal} · {/if}
          {#if grabber.checkTotal}{t('Checking links: {done} of {total}', { done: grabber.checkDone, total: grabber.checkTotal })}{/if}
        </span>
        <span class="flex-1"></span>
        {#if grabber.checkTotal}<span class="mono text-[11px] text-muted">{Math.round(pct)}%</span>{/if}
        <button class="btn !h-[24px] !text-[11.5px]" onclick={() => grabber.cancel()}><Icon name="x" size={11} />{t('Cancel')}</button>
      </div>
      <div class="h-[4px] rounded-full bg-line overflow-hidden">
        {#if grabber.checkTotal}
          <div class="h-full rounded-full bg-accent transition-[width] duration-300" style="width: {pct}%"></div>
        {:else}
          <div class="h-full w-1/3 rounded-full bg-accent/70 animate-pulse"></div>
        {/if}
      </div>
    </div>
  {/if}

  <div class="table-head grid {cols} px-3 h-[30px] items-center text-[11px] font-semibold text-dim border-b border-line flex-shrink-0">
    <span></span><span>{t('Name')}</span><span class="text-right pr-3">{t('Size')}</span><span>{t('Hoster')}</span><span>{t('Found in')}</span><span>{t('Availability')}</span>
  </div>

  <div class="flex-1 overflow-y-auto min-h-0">
    {#each packages as p (p.name)}
      {@const open = !collapsed.has(p.name)}
      {@const allOn = p.links.every((l) => selected.has(l.id))}
      <div class="grid {cols} px-3 min-h-[32px] items-center text-[12.5px] border-b border-line/50 bg-panel/60 hover:bg-hover">
        <input type="checkbox" class="check" checked={allOn} onchange={(e) => selectPackage(p.links, e.currentTarget.checked)} />
        <button type="button" class="flex items-center gap-2 min-w-0 text-left font-medium cursor-pointer" onclick={() => (collapsed = flip(collapsed, p.name))}>
          <Icon name={open ? 'chevronDown' : 'chevron'} size={11} class="text-muted flex-shrink-0" />
          <Icon name="folder" size={14} class="text-accent flex-shrink-0" />
          <span class="truncate" title={p.name}>{p.name}</span>
          <span class="text-dim text-[11px] flex-shrink-0">[{p.links.length}]</span>
        </button>
        <span class="text-right pr-3 mono text-[11.5px]">{p.size ? bytes(p.size) : p.unknownSize ? t('Unknown') : '—'}</span>
        <span class="text-[11.5px] text-muted truncate" title={p.hosts.join(', ')}>{p.hosts.join(', ')}</span>
        <span></span>
        <span class="text-[11.5px] flex items-center gap-1.5 {p.checking ? 'text-muted' : p.offline === p.links.length ? 'text-bad' : p.online === p.links.length ? 'text-ok' : 'text-warn'}">
          <Icon name={p.checking ? 'refresh' : p.offline === p.links.length ? 'x' : 'check'} size={11} class={p.checking ? 'animate-spin' : ''} />{t('{online}/{total} online', { online: p.online, total: p.links.length })}
        </span>
      </div>
      {#if open}
        {#each p.links as l (l.id)}
          {@const type = fileType(l.name, l.url)}
          <div class="grid {cols} px-3 min-h-[28px] items-center text-[12px] border-b border-line/30 hover:bg-hover {selected.has(l.id) ? 'bg-accent/10' : ''}">
            <input type="checkbox" class="check" checked={selected.has(l.id)} onchange={() => (selected = flip(selected, l.id))} />
            <span class="flex items-center gap-2 min-w-0 pl-7">
              <Icon name={TYPE_LABELS[type].icon} size={12} class="text-muted flex-shrink-0" />
              <span class="truncate" title={l.url}>{l.name}</span>
              {#if l.text && l.text !== l.name}<span class="text-[11px] text-dim truncate min-w-0" title={l.text}>— {l.text}</span>{/if}
            </span>
            <span class="text-right pr-3 mono text-[11px] text-muted">{l.size != null ? bytes(l.size) : l.status === 'checking' ? '…' : type === 'page' ? '—' : t('Unknown')}</span>
            <span class="text-[11px] text-muted truncate" title={l.host}>{l.host}</span>
            <span class="text-[11px] text-dim truncate">{l.tag ? t(tagLabel[l.tag] ?? 'Link') : t('Added')}</span>
            <span class="text-[11px] flex items-center gap-1.5 min-w-0 {statusTone[l.status]}" title={l.error ?? ''}>
              <Icon name={statusIcon[l.status]} size={11} class="flex-shrink-0 {l.status === 'checking' ? 'animate-spin' : ''}" />
              <span class="truncate">{l.status === 'online' ? t('Online') : l.status === 'offline' ? t('Offline') : l.status === 'checking' ? t('Checking…') : type === 'page' ? t('Web page') : t('Unknown')}</span>
            </span>
          </div>
        {/each}
      {/if}
    {:else}
      <div class="h-full flex flex-col items-center justify-center gap-3 text-center px-8">
        <div class="w-[56px] h-[56px] rounded-full bg-accent/15 text-accent flex items-center justify-center"><Icon name="link" size={24} /></div>
        <div class="text-[14px] font-semibold">{all.length ? t('No links match the filters') : t('Collect links before downloading')}</div>
        <p class="text-[12px] text-muted max-w-[480px]">{t('Paste links or a web page address. Pages are scanned and every file they link to is listed, grouped into packages. Each link is checked, so you see its size and whether it is still online.')}</p>
        {#if all.length && activeFilters}
          <button class="btn" onclick={resetFilters}>{t('Show everything')}</button>
        {:else if !all.length}
          <button class="btn btn-accent" onclick={openAdd}><Icon name="plus" size={12} />{t('Add new links')}</button>
        {/if}
      </div>
    {/each}
  </div>

  <div class="border-t border-line flex-shrink-0">
    <button type="button" class="w-full px-4 h-[34px] flex items-center gap-2 text-[12px] font-medium text-muted hover:text-white hover:bg-hover cursor-pointer" onclick={() => setPref('optionsOpen', !prefs.optionsOpen)}>
      <Icon name="filter" size={12} />{t('Filters & options')}
      {#if activeFilters}<span class="text-[10.5px] px-1.5 rounded-full bg-accent/20 text-accent">{t('{count} hidden', { count: activeFilters })}</span>{/if}
      <span class="flex-1"></span>
      <span class="text-[11px] text-dim font-normal">{t('{shown} of {total} links shown', { shown: stats.shown, total: stats.links })}</span>
      <Icon name={prefs.optionsOpen ? 'chevronDown' : 'chevronUp'} size={12} />
    </button>
    {#if prefs.optionsOpen}
      <div class="max-h-[34vh] overflow-y-auto px-4 pb-3 pt-1 grid grid-cols-[minmax(0,1fr)_260px] gap-x-6 gap-y-3 border-t border-line/50">
        <div class="space-y-3 min-w-0">
          <div>
            <div class="flex items-center justify-between pb-1.5">
              <span class="text-[10.5px] font-semibold text-dim uppercase tracking-wider">{t('Show these file types')}</span>
              {#if activeFilters}<button class="text-[11px] text-accent hover:underline cursor-pointer" onclick={resetFilters}>{t('Show everything')}</button>{/if}
            </div>
            <div class="flex flex-wrap gap-1.5">
              {#each Object.entries(TYPE_LABELS) as [id, meta] (id)}
                {#if typeCounts.get(id as FileType)}
                  {@render chip(!hiddenTypes.has(id as FileType), t(meta.label), typeCounts.get(id as FileType) ?? 0, meta.icon, () => toggleType(id as FileType))}
                {/if}
              {/each}
              {#if !typeCounts.size}<span class="text-[11.5px] text-dim">{t('Nothing collected yet.')}</span>{/if}
            </div>
          </div>
          {#if extCounts.length}
            <div>
              <div class="text-[10.5px] font-semibold text-dim uppercase tracking-wider pb-1.5">{t('Extensions')}</div>
              <div class="flex flex-wrap gap-1.5">
                {#each extCounts.slice(0, 60) as [e, n] (e)}
                  {@render chip(!hiddenExts.has(e), '.' + e, n, null, () => (hiddenExts = flip(hiddenExts, e)))}
                {/each}
              </div>
            </div>
          {/if}
          {#if hostCounts.length}
            <div>
              <div class="text-[10.5px] font-semibold text-dim uppercase tracking-wider pb-1.5">{t('Hosters')}</div>
              <div class="flex flex-wrap gap-1.5">
                {#each hostCounts.slice(0, 60) as [h, n] (h)}
                  {@render chip(!hiddenHosts.has(h), h || '—', n, 'globe', () => (hiddenHosts = flip(hiddenHosts, h)))}
                {/each}
              </div>
            </div>
          {/if}
        </div>
        <div class="space-y-0.5 border-l border-line/50 pl-5">
          <div class="text-[10.5px] font-semibold text-dim uppercase tracking-wider pb-1">{t('Options')}</div>
          {@render switchRow(prefs.showOffline, t('Show offline links'), '', () => setPref('showOffline', !prefs.showOffline))}
          {@render switchRow(prefs.sameSite, t('Only files from the same site'), t('Hide links that point to other websites.'), () => setPref('sameSite', !prefs.sameSite))}
          {@render switchRow(prefs.scanPages, t('Scan web pages for links'), t('A page address lists every file on that page.'), () => setPref('scanPages', !prefs.scanPages))}
          {@render switchRow(prefs.scanSubpages, t('Also scan linked pages on the same site'), t('One level deep, up to the limit below.'), () => setPref('scanSubpages', !prefs.scanSubpages))}
          {@render switchRow(prefs.autoCheck, t('Check links automatically'), t('Finds the size and whether each file is online.'), () => setPref('autoCheck', !prefs.autoCheck))}
        </div>
        <div class="col-span-2 border-t border-line/50 pt-3">
          <div class="flex items-center justify-between pb-1.5">
            <span class="text-[10.5px] font-semibold text-dim uppercase tracking-wider">{t('Speed')}</span>
            {#if speedChanged}<button class="text-[11px] text-accent hover:underline cursor-pointer" onclick={resetSpeed}>{t('Back to the defaults')}</button>{/if}
          </div>
          <div class="text-[11.5px] text-muted pb-1.5">{t('Check these file types automatically. The others are listed without checking, which is much faster on big pages; Check again checks them on demand.')}</div>
          <div class="flex flex-wrap gap-1.5 pb-3 {prefs.autoCheck ? '' : 'opacity-45 pointer-events-none'}">
            {#each Object.entries(TYPE_LABELS) as [id, meta] (id)}
              {@render chip(prefs.checkTypes.includes(id as FileType), t(meta.label), typeCounts.get(id as FileType) ?? 0, meta.icon, () => toggleCheckType(id as FileType))}
            {/each}
          </div>
          <div class="grid grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-3 text-[12px]">
            <label class="flex flex-col gap-1">
              <span class="text-muted">{t('Links checked at the same time')}</span>
              <input type="number" min="1" max="32" class="field !h-[30px] !w-[90px] text-[12px]" value={prefs.parallel} onchange={(e) => setNum('parallel', e.currentTarget.valueAsNumber, 1, 32)} />
            </label>
            <label class="flex flex-col gap-1">
              <span class="text-muted">{t('Wait for a page (seconds)')}</span>
              <input type="number" min="3" max="300" class="field !h-[30px] !w-[90px] text-[12px]" value={prefs.pageTimeout} onchange={(e) => setNum('pageTimeout', e.currentTarget.valueAsNumber, 3, 300)} />
            </label>
            <label class="flex flex-col gap-1">
              <span class="text-muted">{t('Linked pages to scan at most')}</span>
              <input type="number" min="1" max="200" class="field !h-[30px] !w-[90px] text-[12px]" value={prefs.subpageLimit} onchange={(e) => setNum('subpageLimit', e.currentTarget.valueAsNumber, 1, 200)} />
            </label>
          </div>
        </div>
      </div>
    {/if}
  </div>

  <div class="border-t border-line px-4 py-2 flex flex-wrap gap-1.5 text-[11.5px] flex-shrink-0">
    <span class="stat-pill text-muted">{t('Packages')} <b class="text-white font-semibold">{stats.packages}</b></span>
    <span class="stat-pill text-muted">{t('Links')} <b class="text-white font-semibold">{stats.links}</b></span>
    <span class="stat-pill text-muted">{t('Total size')} <b class="text-white font-semibold">{bytes(stats.bytes)}</b></span>
    <span class="stat-pill text-muted">{t('Hosters')} <b class="text-white font-semibold">{stats.hosts}</b></span>
    <span class="stat-pill stat-ok">{t('Online')} <b class="font-semibold">{stats.online}</b></span>
    <span class="stat-pill stat-bad">{t('Offline')} <b class="font-semibold">{stats.offline}</b></span>
    <span class="stat-pill stat-warn">{t('Unknown')} <b class="font-semibold">{stats.unknown}</b></span>
    {#if stats.checking}<span class="text-muted flex items-center gap-1"><Icon name="refresh" size={10} class="animate-spin" />{t('Checking…')} {stats.checking}</span>{/if}
  </div>

  <div class="border-t border-line px-4 py-2.5 flex items-center gap-2 flex-shrink-0">
    <span class="text-[11.5px] text-muted whitespace-nowrap">{t('Save to')}</span>
    <input class="field !h-[30px] flex-1 min-w-0 text-[12px]" bind:value={dir} />
    <button class="btn !h-[30px]" onclick={() => browse((v) => (dir = v), dir)}><Icon name="folder" size={12} /></button>
    <label class="flex items-center gap-1.5 text-[11.5px] text-muted whitespace-nowrap"><input type="checkbox" class="check" bind:checked={paused} />{t('Add paused')}</label>
    <button class="btn btn-primary !h-[30px] !px-4 whitespace-nowrap" disabled={!startable.length || busy} onclick={startLinks}>
      <Icon name="down" size={12} />{busy ? '…' : chosen.length ? t('Start selected ({count})', { count: startable.length }) : t('Start all downloads')}
    </button>
  </div>
</div>

{#if grabber.addOpen}
  <div class="fixed inset-0 bg-black/55 backdrop-blur-sm z-50 flex items-center justify-center p-6" role="presentation" onclick={(e) => e.target === e.currentTarget && closeAdd()}>
    <div class="w-full max-w-[720px] bg-menu border border-line rounded-[14px] shadow-2xl flex flex-col max-h-[92vh]">
      <div class="px-5 h-[50px] border-b border-line flex items-center gap-2.5 flex-shrink-0">
        <Icon name="search" size={15} class="text-accent" />
        <span class="text-[14px] font-semibold flex-1">{t('Analyse and add links')}</span>
        <button class="icon-btn" aria-label={t('Close')} onclick={closeAdd}><Icon name="x" size={14} /></button>
      </div>
      <div class="p-5 space-y-3 overflow-y-auto">
        <textarea class="field !h-[150px] py-2 mono text-[11.5px] resize-y" placeholder={t('Enter links, web page addresses or any text that contains links…')} bind:value={pasteText}></textarea>
        <div class="flex items-center justify-between text-[11.5px] text-dim">
          <span>{pasted.length ? t('{count} links recognised', { count: pasted.length }) : t('A number range like file[01-20].zip adds every number.')}</span>
          <label class="flex items-center gap-1.5 cursor-pointer text-muted"><input type="checkbox" class="check" checked={prefs.scanPages} onchange={() => setPref('scanPages', !prefs.scanPages)} />{t('Scan web pages for links')}</label>
        </div>

        <div class="grid grid-cols-[24px_minmax(0,1fr)] gap-x-3 gap-y-2.5 items-center pt-1">
          <Icon name="folder" size={15} class="text-muted" />
          <div class="flex gap-2"><input class="field flex-1 min-w-0" placeholder={t('Save to')} bind:value={addDir} /><button class="btn" onclick={() => browse((v) => (addDir = v), addDir)}>{t('Browse')}</button></div>

          <Icon name="archive" size={15} class="text-muted" />
          <input class="field" placeholder={t('Package name, or leave empty to name it automatically')} bind:value={pkgName} />

          <Icon name="edit" size={15} class="text-muted" />
          <input class="field" placeholder={t('Comment, or leave empty')} bind:value={note} />

          <Icon name="lock" size={15} class="text-muted" />
          <div class="flex items-center gap-3">
            <input class="field flex-1 min-w-0" type="password" autocomplete="off" placeholder={t('Password to extract the archives')} bind:value={extractPassword} />
            <label class="flex items-center gap-1.5 text-[12px] text-muted whitespace-nowrap cursor-pointer"><input type="checkbox" class="check" bind:checked={autoExtract} />{t('Auto extract')}</label>
          </div>

          <Icon name="users" size={15} class="text-muted" />
          <div class="flex items-center gap-2">
            <input class="field flex-1 min-w-0" autocomplete="off" placeholder={t('User name for protected links')} bind:value={loginUser} />
            <input class="field flex-1 min-w-0" type="password" autocomplete="off" placeholder={t('Password for protected links')} bind:value={loginPassword} />
          </div>

          <Icon name="queue" size={15} class="text-muted" />
          <div class="flex items-center gap-2">
            <select class="field !w-auto" bind:value={priority}>
              <option value="normal">{t('Normal priority')}</option>
              <option value="high">{t('High priority (start first)')}</option>
              <option value="low">{t('Low priority (start last)')}</option>
            </select>
            <span class="text-[11px] text-dim">{t('Applies to these links when you start them.')}</span>
          </div>
        </div>
        {#if loginPassword && !loginUser.trim()}
          <p class="text-[11.5px] text-warn">{t('Enter a user name too, or the password is not used.')}</p>
        {/if}
        {#if loginPassword && pasted.some((u) => u.toLowerCase().startsWith('http://'))}
          <p class="text-[11.5px] text-warn">{t('Passwords are only sent over encrypted (HTTPS) links.')}</p>
        {/if}
      </div>
      <div class="px-5 py-3 border-t border-line flex items-center justify-end gap-2 flex-shrink-0">
        <button class="btn" onclick={closeAdd}>{t('Cancel')}</button>
        <button class="btn btn-primary !px-5" disabled={!pasted.length || analysing} onclick={analyse}>{analysing ? '…' : t('Continue')}</button>
      </div>
    </div>
  </div>
{/if}
