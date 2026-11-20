<script lang="ts">
  import { tick } from 'svelte';
  import { ask } from '@tauri-apps/plugin-dialog';
  import { app } from '../lib/store.svelte';
  import { t } from '../lib/i18n.svelte';
  import { bytes } from '../lib/format';
  import Icon from './Icon.svelte';
  import CategoryDialog from './CategoryDialog.svelte';
  import { tagHue } from '../lib/tags';
  import { grabber } from '../lib/grabber.svelte';
  import type { CategoryFilter, KindFilter, SidebarGroup, StatusFilter } from '../types';

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
  const BUILT_IN: Record<string, string> = { sources: 'Sources', categories: 'Categories', tags: 'Tags', tools: 'Tools' };
  const TOOL_IDS = ['grabber', 'search', 'rss', 'stats', 'history', 'create', 'extension'];

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

  let tagCounts = $derived.by(() => {
    const m = new Map<string, number>();
    for (const d of app.downloads) for (const x of d.tags ?? []) m.set(x, (m.get(x) ?? 0) + 1);
    return [...m.entries()].sort((a, b) => a[0].localeCompare(b[0]));
  });

  function pickTag(tag: string) {
    app.view = 'downloads';
    app.tagFilter = app.tagFilter === tag ? null : tag;
  }

  type Row = { id: string; label: string; short: string; icon: string; on: boolean; go: () => void; count?: number; dot?: 'ok' | 'warn'; edit?: number; title?: string; tool: boolean };
  const toggleView = (v: typeof app.view) => () => (app.view = app.view === v ? 'downloads' : v);

  let rows = $derived.by(() => {
    const map: Record<string, Row> = {};
    for (const k of kinds) {
      map[`kind:${k.id}`] = { id: `kind:${k.id}`, label: t(k.label), short: t(k.label), icon: k.icon, on: app.view === 'downloads' && app.kindFilter === k.id, go: () => pickKind(k.id), count: app.counts[`kind:${k.id}`] ?? 0, tool: false };
    }
    for (const c of categories) {
      map[`cat:${c.id}`] = { id: `cat:${c.id}`, label: t(c.label), short: t(c.label), icon: c.icon, on: app.view === 'downloads' && app.categoryFilter === c.id, go: () => pickCategory(c.id), count: app.counts[`cat:${c.id}`] || undefined, tool: false };
    }
    (app.settings?.customCategories ?? []).forEach((c, i) => {
      const id = `custom:${c.name}` as CategoryFilter;
      map[`cat:${id}`] = { id: `cat:${id}`, label: c.name, short: c.name, icon: c.icon || 'folder', on: app.view === 'downloads' && app.categoryFilter === id, go: () => pickCategory(id), count: app.counts[`cat:${id}`] || undefined, edit: i, title: c.extensions.map((e) => '.' + e).join(' '), tool: false };
    });
    const tool = (id: string, label: string, short: string, icon: string, on: boolean, go: () => void, extra: Partial<Row> = {}) => {
      map[id] = { id, label: t(label), short: id === 'rss' ? 'RSS' : t(short), icon, on, go, tool: true, ...extra };
    };
    tool('grabber', 'LinkGrabber', 'Links', 'link', app.view === 'grabber', toggleView('grabber'), { count: grabber.links.length || undefined });
    tool('search', 'Torrent search', 'Search', 'search', app.view === 'search', toggleView('search'));
    tool('rss', 'RSS Feeds', 'RSS', 'rss', app.view === 'rss', toggleView('rss'));
    if (app.settings?.showStats !== false) tool('stats', 'Statistics', 'Stats', 'gauge', app.view === 'stats', toggleView('stats'));
    if (app.settings?.showHistory !== false && app.settings?.historyEnabled !== false) tool('history', 'Download history', 'History', 'clock', app.view === 'history', toggleView('history'));
    tool('create', 'Create torrent', 'Create', 'magnet', false, () => (app.createTorrentOpen = true));
    tool('extension', 'Browser extension', 'Extension', 'puzzle', false, () => (app.extensionSetupOpen = true), { dot: app.settings?.trustedExtensions.length ? 'ok' : 'warn' });
    return map;
  });

  let allItems = $derived.by(() => {
    const custom = (app.settings?.customCategories ?? []).map((c) => `cat:custom:${c.name}`);
    const cats = [...categories.map((c) => `cat:${c.id}`), ...custom];
    const order = (app.settings?.categoryOrder ?? []).map((o) => `cat:${o}`);
    const rank = (id: string) => (order.indexOf(id) === -1 ? order.length + cats.indexOf(id) : order.indexOf(id));
    return {
      sources: kinds.map((k) => `kind:${k.id}`),
      categories: [...cats].sort((a, b) => rank(a) - rank(b)),
      tools: TOOL_IDS
    } as Record<string, string[]>;
  });

  function normalize(saved: SidebarGroup[] | undefined): SidebarGroup[] {
    const known = new Set(Object.values(allItems).flat());
    const out: SidebarGroup[] = [];
    const groups = new Set<string>();
    const seen = new Set<string>();
    for (const g of saved ?? []) {
      if (groups.has(g.id) || !(g.id in BUILT_IN || g.id.startsWith('g-'))) continue;
      groups.add(g.id);
      const items = g.id === 'tags' ? [] : (g.items ?? []).filter((i) => known.has(i) && !seen.has(i));
      items.forEach((i) => seen.add(i));
      out.push({ id: g.id, title: g.title ?? '', items, collapsed: !!g.collapsed });
    }
    for (const id of Object.keys(BUILT_IN)) if (!groups.has(id)) out.push({ id, title: '', items: [], collapsed: false });
    for (const [home, ids] of Object.entries(allItems)) {
      const g = out.find((x) => x.id === home)!;
      for (const i of ids) if (!seen.has(i)) g.items.push(i);
    }
    return out;
  }

  let layout = $derived(normalize(app.settings?.sidebarLayout));
  const acceptsItems = (g: SidebarGroup) => g.id !== 'tags';
  const titleOf = (g: SidebarGroup) => (g.id in BUILT_IN ? t(BUILT_IN[g.id]) : g.title || t('New group'));
  const visibleRows = (g: SidebarGroup) => g.items.map((i) => rows[i]).filter(Boolean);
  const shown = (g: SidebarGroup) => (g.id === 'tags' ? app.settings?.showTags !== false && tagCounts.length > 0 : true);
  let tiles = $derived(app.settings?.sidebarShortcuts === 'tiles');
  const tileGroup = (list: Row[]) => tiles && list.length > 0 && list.every((r) => r.tool);

  async function saveLayout(next: SidebarGroup[]) {
    if (!app.settings) return;
    await app.saveSettings({ ...app.settings, sidebarLayout: next });
  }

  function toggleGroup(id: string) {
    if (justDragged) return;
    saveLayout(layout.map((g) => (g.id === id ? { ...g, collapsed: !g.collapsed } : g)));
  }

  let editing = $state<string | null>(null);
  let editText = $state('');
  let editInput = $state<HTMLInputElement>();

  async function newGroup() {
    const id = 'g-' + Date.now().toString(36);
    await saveLayout([...layout, { id, title: t('New group'), items: [], collapsed: false }]);
    startRename(id, t('New group'));
  }

  async function startRename(id: string, current: string) {
    editing = id;
    editText = current;
    await tick();
    editInput?.select();
  }

  function finishRename() {
    const id = editing;
    editing = null;
    if (!id) return;
    const title = editText.trim().slice(0, 40);
    if (title) saveLayout(layout.map((g) => (g.id === id ? { ...g, title } : g)));
  }

  function removeGroup(id: string) {
    const gone = layout.find((g) => g.id === id);
    if (!gone) return;
    saveLayout(layout.filter((g) => g.id !== id));
  }

  async function resetLayout() {
    const yes = await ask(t('Reset the sidebar layout? Your own groups are removed and every item goes back to its original group and order. Downloads and categories are not changed.'), {
      title: t('Reset the sidebar layout'),
      kind: 'warning',
      okLabel: t('Reset'),
      cancelLabel: t('Cancel')
    }).catch(() => false);
    if (yes) saveLayout([]);
  }

  let justDragged = false;
  function markDragged() {
    justDragged = true;
    setTimeout(() => (justDragged = false), 0);
  }

  function overIndex(els: (HTMLElement | undefined)[], y: number) {
    for (let i = 0; i < els.length; i++) {
      const r = els[i]?.getBoundingClientRect();
      if (r && r.height && y < r.top + r.height / 2) return i;
    }
    return els.length;
  }

  let groupEls = $state<HTMLElement[]>([]);
  let gdrag = $state<{ from: number; over: number; startY: number; dy: number; moved: boolean } | null>(null);

  function startGroupDrag(e: PointerEvent, i: number) {
    if (e.button !== 0) return;
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    gdrag = { from: i, over: i, startY: e.clientY, dy: 0, moved: false };
  }

  function moveGroupDrag(e: PointerEvent) {
    if (!gdrag) return;
    gdrag.dy = e.clientY - gdrag.startY;
    if (Math.abs(gdrag.dy) > 5) gdrag.moved = true;
    if (gdrag.moved) gdrag.over = overIndex(groupEls, e.clientY);
  }

  function endGroupDrag() {
    const d = gdrag;
    gdrag = null;
    if (!d || !d.moved) return;
    markDragged();
    const target = d.over > d.from ? d.over - 1 : d.over;
    if (target === d.from) return;
    const next = [...layout];
    const [moved] = next.splice(d.from, 1);
    next.splice(target, 0, moved);
    saveLayout(next);
  }

  let idrag = $state<{ id: string; from: string; startX: number; startY: number; x: number; y: number; moved: boolean; to: string | null; index: number } | null>(null);

  function startItemDrag(e: PointerEvent, id: string, from: string) {
    if (e.button !== 0) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    idrag = { id, from, startX: e.clientX, startY: e.clientY, x: e.clientX, y: e.clientY, moved: false, to: from, index: 0 };
  }

  function moveItemDrag(e: PointerEvent) {
    if (!idrag) return;
    idrag.x = e.clientX;
    idrag.y = e.clientY;
    if (Math.hypot(e.clientX - idrag.startX, e.clientY - idrag.startY) > 5) idrag.moved = true;
    if (!idrag.moved) return;
    idrag.to = null;
    let best: { gi: number; dist: number } | null = null;
    for (let gi = 0; gi < layout.length; gi++) {
      const el = groupEls[gi];
      if (!el || !acceptsItems(layout[gi])) continue;
      const r = el.getBoundingClientRect();
      if (!r.height) continue;
      const dist = e.clientY < r.top ? r.top - e.clientY : e.clientY > r.bottom ? e.clientY - r.bottom : 0;
      if (!best || dist < best.dist) best = { gi, dist };
    }
    if (!best || best.dist > 40) return;
    const g = layout[best.gi];
    const el = groupEls[best.gi]!;
    idrag.to = g.id;
    const items = [...el.querySelectorAll<HTMLElement>('[data-item]')].filter((x) => x.dataset.item !== idrag!.id);
    let index = items.length;
    for (let i = 0; i < items.length; i++) {
      const b = items[i].getBoundingClientRect();
      if (e.clientY < b.top || (e.clientY <= b.bottom && (b.width < 150 ? e.clientX < b.left + b.width / 2 : e.clientY < b.top + b.height / 2))) {
        index = i;
        break;
      }
    }
    idrag.index = index;
  }

  function endItemDrag() {
    const d = idrag;
    idrag = null;
    if (!d || !d.moved) return;
    markDragged();
    if (!d.to) return;
    const next = layout.map((g) => ({ ...g, items: g.items.filter((i) => i !== d.id) }));
    const target = next.find((g) => g.id === d.to);
    if (!target) return;
    const vis = target.items.filter((i) => rows[i]);
    const before = vis[d.index];
    const at = before ? target.items.indexOf(before) : target.items.length;
    target.items.splice(at, 0, d.id);
    target.collapsed = false;
    saveLayout(next);
  }

  function clickRow(row: Row) {
    if (justDragged) return;
    row.go();
  }

  let usedPct = $derived(app.disk ? ((app.disk.total - app.disk.free) / app.disk.total) * 100 : 0);
  const rowClass = (on: boolean) => `glow-target w-full h-[30px] px-[10px] rounded-[8px] flex items-center justify-between text-[12.5px] transition-colors cursor-pointer ${on ? 'bg-line text-white' : 'text-muted hover:bg-hover hover:text-white'}`;
</script>

{#snippet dropLine()}
  <div class="mx-2 my-[1px] h-[2px] rounded-full bg-accent"></div>
{/snippet}

{#snippet itemRow(row: Row, gid: string, i: number, asTile: boolean)}
  {@const lifted = idrag?.moved && idrag.id === row.id}
  {@const target = idrag?.moved && idrag.to === gid && idrag.index === i}
  {#if target && !asTile}{@render dropLine()}{/if}
  <div class="group relative" data-item={row.id}>
    <button
      type="button"
      title={row.title || t('Drag to move it to another group')}
      onpointerdown={(e) => startItemDrag(e, row.id, gid)}
      onpointermove={moveItemDrag}
      onpointerup={endItemDrag}
      onpointercancel={() => (idrag = null)}
      onclick={() => clickRow(row)}
      class="{asTile
        ? `sidebar-tile relative w-full h-[50px] rounded-[10px] flex flex-col items-center justify-center gap-[3px] text-[11px] font-medium cursor-pointer transition-colors min-w-0 px-1 ${row.on ? 'sidebar-tile-on' : 'text-muted hover:text-white'} ${target ? 'ring-2 ring-accent' : ''}`
        : rowClass(row.on)} select-none touch-none {lifted ? 'opacity-30' : ''}"
    >
      {#if asTile}
        <Icon name={row.icon} size={16} /><span class="truncate max-w-full">{row.short}</span>
        {#if row.dot}<span class="absolute top-1.5 right-1.5 w-[6px] h-[6px] rounded-full {row.dot === 'ok' ? 'bg-ok' : 'bg-warn'}"></span>{/if}
      {:else}
        <span class="flex items-center gap-[10px] min-w-0"><Icon name={row.icon} size={15} /><span class="truncate">{row.label}</span></span>
        {#if row.dot}
          <span class="w-[7px] h-[7px] rounded-full {row.dot === 'ok' ? 'bg-ok' : 'bg-warn'}" title={row.dot === 'ok' ? t('Connected') : t('Not connected')}></span>
        {:else if row.count !== undefined}
          <span class="text-[11px] text-dim {row.edit !== undefined ? 'group-hover:opacity-0' : ''}">{row.count}</span>
        {/if}
      {/if}
    </button>
    {#if row.edit !== undefined && !idrag?.moved && !asTile}
      <button type="button" class="icon-btn !w-5 !h-5 absolute right-[6px] top-[5px] opacity-0 group-hover:opacity-100" title={t('Edit category')} aria-label={t('Edit category')} onclick={() => (app.categoryDialog = { index: row.edit! })}><Icon name="edit" size={11} /></button>
    {/if}
  </div>
{/snippet}

<aside class="w-[220px] bg-panel border-r border-line flex flex-col p-[14px_12px] flex-shrink-0 h-full">

  <div class="flex-1 overflow-y-auto pr-1">
    <nav class="space-y-[2px] pb-4">
      {#each statuses as s}
        {@const on = app.view === 'downloads' && app.statusFilter === s.id && app.kindFilter === 'all' && app.categoryFilter === 'all'}
        <button
          type="button"
          onclick={() => pickStatus(s.id)}
          class="glow-target w-full h-[32px] px-[10px] rounded-[8px] flex items-center justify-between text-[12.5px] font-medium transition-colors cursor-pointer {on ? 'bg-line text-white' : 'text-muted hover:bg-hover hover:text-white'}"
        >
          <span class="flex items-center gap-[10px]"><Icon name={s.icon} size={15} />{t(s.label)}</span>
          <span class="flex items-center gap-1.5 text-[11px] {on ? 'text-white' : 'text-dim'}">{#if s.id === 'active' && (app.counts.active ?? 0) > 0}<span class="live-dot"></span>{/if}{#if s.id === 'all' && (app.counts.active ?? 0) > 0}<span class="live-dot live-dot-green" title={t('Downloading')}></span>{/if}{#key app.counts[s.id] ?? 0}<span class="count-bump">{app.counts[s.id] ?? 0}</span>{/key}</span>
        </button>
      {/each}
    </nav>

    {#each layout as g, gi (g.id)}
      {@const lifted = gdrag?.moved && gdrag.from === gi}
      {@const list = visibleRows(g)}
      {@const asTiles = tileGroup(list)}
      {#if gdrag?.moved && gdrag.over === gi && gi !== gdrag.from && gi !== gdrag.from + 1}{@render dropLine()}{/if}
      <section
        bind:this={groupEls[gi]}
        class="relative rounded-[10px] {shown(g) || (acceptsItems(g) && idrag?.moved) ? 'pb-3' : 'hidden'} {lifted ? 'z-20 bg-raised shadow-lg' : ''} {idrag?.moved && idrag.to === g.id && idrag.from !== g.id ? 'bg-accent/8 ring-1 ring-accent/40' : ''}"
        style={lifted ? `transform: translateY(${gdrag?.dy ?? 0}px)` : ''}
      >
        <div class="group/h flex items-center h-[26px] pr-1">
          <button
            type="button"
            class="w-[14px] h-[20px] -ml-[2px] flex items-center justify-center text-dim opacity-0 group-hover/h:opacity-100 cursor-grab active:cursor-grabbing touch-none select-none {lifted ? 'opacity-100' : ''}"
            title={t('Drag to reorder')}
            aria-label={t('Drag to reorder')}
            onpointerdown={(e) => startGroupDrag(e, gi)}
            onpointermove={moveGroupDrag}
            onpointerup={endGroupDrag}
            onpointercancel={() => (gdrag = null)}
          ><Icon name="grip" size={12} /></button>
          {#if editing === g.id}
            <input
              bind:this={editInput}
              class="flex-1 min-w-0 h-[22px] px-1.5 bg-bg border border-accent rounded-[5px] text-[11px] outline-none"
              bind:value={editText}
              maxlength="40"
              onblur={finishRename}
              onkeydown={(e) => {
                if (e.key === 'Enter') finishRename();
                if (e.key === 'Escape') editing = null;
              }}
            />
          {:else}
            <button type="button" class="sidebar-heading flex-1 min-w-0 flex items-center gap-1 text-left cursor-pointer" onclick={() => toggleGroup(g.id)} title={g.collapsed ? t('Show') : t('Hide')}>
              <h3 class="text-[10.5px] font-semibold uppercase tracking-wider truncate">{titleOf(g)}</h3>
              <Icon name={g.collapsed ? 'chevron' : 'chevronDown'} size={10} class="flex-shrink-0 opacity-0 group-hover/h:opacity-100 {g.collapsed ? '!opacity-100' : ''}" />
              {#if g.collapsed}
                <span class="text-[10.5px] text-dim/80 ml-auto pr-1">{g.id === 'tags' ? tagCounts.length : list.length}</span>
              {/if}
            </button>
          {/if}
          {#if g.id === 'categories'}
            <button type="button" class="icon-btn !w-5 !h-5 text-dim hover:!text-white" title={t('Add category')} aria-label={t('Add category')} onclick={() => (app.categoryDialog = { index: null })}><Icon name="plus" size={12} /></button>
          {:else if g.id.startsWith('g-') && editing !== g.id}
            <button type="button" class="icon-btn !w-5 !h-5 text-dim hover:!text-white opacity-0 group-hover/h:opacity-100" title={t('Rename group')} aria-label={t('Rename group')} onclick={() => startRename(g.id, g.title)}><Icon name="edit" size={11} /></button>
            <button type="button" class="icon-btn !w-5 !h-5 text-dim hover:!text-bad opacity-0 group-hover/h:opacity-100" title={t('Remove group')} aria-label={t('Remove group')} onclick={() => removeGroup(g.id)}><Icon name="x" size={11} /></button>
          {/if}
        </div>

        {#if !g.collapsed}
          {#if g.id === 'tags'}
            <div class="space-y-[2px]">
              {#each tagCounts as [tag, n] (tag)}
                <button type="button" onclick={() => pickTag(tag)} class={rowClass(app.tagFilter === tag && app.view === 'downloads')}>
                  <span class="flex items-center gap-[10px] min-w-0"><span class="w-[9px] h-[9px] rounded-full flex-shrink-0" style="background: hsl({tagHue(tag)} 70% 58%)"></span><span class="truncate">{tag}</span></span>
                  <span class="text-[11px] text-dim">{n}</span>
                </button>
              {/each}
            </div>
          {:else}
            <div class={asTiles ? 'grid grid-cols-2 gap-1.5' : 'space-y-[2px]'}>
              {#each list as row, i (row.id)}
                {@render itemRow(row, g.id, i, asTiles)}
              {/each}
              {#if idrag?.moved && idrag.to === g.id && idrag.index >= list.filter((r) => r.id !== idrag?.id).length && !asTiles}{@render dropLine()}{/if}
              {#if !list.length}
                <div class="h-[34px] rounded-[8px] border border-dashed border-line text-[11px] text-dim flex items-center justify-center">{t('Drag items here')}</div>
              {/if}
            </div>
          {/if}
        {/if}
      </section>
    {/each}
    {#if gdrag?.moved && gdrag.over === layout.length && gdrag.from !== layout.length - 1}{@render dropLine()}{/if}

    <div class="flex items-center gap-1 mb-1">
      <button type="button" class="flex-1 h-[28px] px-[10px] rounded-[8px] flex items-center gap-2 text-[11.5px] text-dim hover:text-white hover:bg-hover cursor-pointer" onclick={newGroup}>
        <Icon name="plus" size={12} />{t('New group')}
      </button>
      {#if app.settings?.sidebarLayout?.length}
        <button type="button" class="icon-btn !w-7 !h-7 text-dim" title={t('Reset the sidebar layout')} aria-label={t('Reset the sidebar layout')} onclick={resetLayout}><Icon name="refresh" size={12} /></button>
      {/if}
    </div>
  </div>

  <div class="pt-3 mt-2 border-t border-line/60">
    <button
      type="button"
      onclick={() => (app.view = app.view === 'settings' ? 'downloads' : 'settings')}
      class="glow-target w-full h-[32px] px-[10px] rounded-[8px] flex items-center gap-[10px] text-[12.5px] font-medium cursor-pointer {app.view === 'settings' ? 'bg-line text-white' : 'text-muted hover:bg-hover hover:text-white'}"
    >
      <Icon name="settings" size={15} />{t('Settings')}
    </button>
    {#if app.disk}
      <div class="px-1 space-y-[6px] mt-3">
        <div class="flex items-center justify-between text-[11px]">
          <span class="text-muted truncate">{t('Downloads')}</span>
          <span class="text-dim">{bytes(app.disk.free)} {t('free')}</span>
        </div>
        <div class="w-full h-[4px] bg-line rounded-[2px] overflow-hidden">
          <div class="h-full rounded-[2px] transition-[width] duration-700 {usedPct > 90 ? 'bg-bad' : 'disk-fill'}" style="width: {usedPct}%"></div>
        </div>
      </div>
    {/if}
  </div>
</aside>

{#if idrag?.moved}
  {@const row = rows[idrag.id]}
  {#if row}
    <div class="fixed z-[80] pointer-events-none h-[30px] px-3 rounded-[8px] bg-raised border border-accent/50 shadow-2xl flex items-center gap-2 text-[12.5px] text-white" style="left: {idrag.x + 12}px; top: {idrag.y - 15}px">
      <Icon name={row.icon} size={14} />{row.label}
    </div>
  {/if}
{/if}

<CategoryDialog />
