<script lang="ts">
  import { untrack } from 'svelte';
  import { api, errorText } from '../lib/api';
  import { i18n, t } from '../lib/i18n.svelte';
  import { bytes } from '../lib/format';
  import type { ArchiveEntry, ArchiveListing, BrowserCookie } from '../types';
  import Icon from './Icon.svelte';

  let {
    url,
    referrer = null,
    cookies = [],
    listing = $bindable(null),
    selected = $bindable([]),
    folders = $bindable(false),
    onClose
  }: {
    url: string;
    referrer?: string | null;
    cookies?: BrowserCookie[];
    listing?: ArchiveListing | null;
    selected?: ArchiveEntry[];
    folders?: boolean;
    onClose: () => void;
  } = $props();

  const PROBLEMS: Record<string, string> = {
    encrypted: 'Encrypted',
    solid: 'Solid archive',
    method: 'Unsupported compression',
    fragmented: 'Stored in pieces',
    link: 'Link'
  };
  const FORMATS: Record<string, string> = { zip: 'ZIP', iso: 'ISO 9660', udf: 'UDF', tar: 'TAR', rar: 'RAR', sevenzip: '7z' };
  const MAX_ROWS = 1500;

  type Node = { name: string; path: string; entry: ArchiveEntry | null; children: Node[]; files: string[] };

  let loading = $state(false);
  let error = $state<string | null>(null);
  let query = $state('');
  let open = $state<Set<string>>(new Set());
  let picked = $state<Set<string>>(new Set());

  async function load() {
    loading = true;
    error = null;
    listing = null;
    picked = new Set();
    try {
      listing = await api.archiveList(url, referrer, cookies);
      const top = tree(listing.entries);
      open = new Set(top.children.length === 1 && isFolder(top.children[0]) ? [top.children[0].path] : []);
    } catch (e) {
      error = errorText(e);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    url;
    untrack(load);
  });

  function pickable(e: ArchiveEntry) {
    return !e.dir && !e.problem && e.method !== 'unsupported';
  }

  function tree(entries: ArchiveEntry[]): Node {
    const root: Node = { name: '', path: '', entry: null, children: [], files: [] };
    const byPath = new Map<string, Node>([['', root]]);
    const folder = (path: string): Node => {
      let n = byPath.get(path);
      if (n) return n;
      const cut = path.lastIndexOf('/');
      const parent = folder(cut < 0 ? '' : path.slice(0, cut));
      n = { name: path.slice(cut + 1), path, entry: null, children: [], files: [] };
      byPath.set(path, n);
      parent.children.push(n);
      return n;
    };
    for (const e of entries) {
      if (e.dir) {
        folder(e.path).entry = e;
        continue;
      }
      const cut = e.path.lastIndexOf('/');
      const parent = folder(cut < 0 ? '' : e.path.slice(0, cut));
      parent.children.push({ name: e.path.slice(cut + 1), path: e.path, entry: e, children: [], files: [] });
    }
    const fill = (n: Node): string[] => {
      for (const c of n.children) {
        if (c.entry && !c.entry.dir) {
          if (pickable(c.entry)) n.files.push(c.path);
        } else {
          for (const f of fill(c)) n.files.push(f);
        }
      }
      n.children.sort((a, b) => Number(!isFolder(a)) - Number(!isFolder(b)) || a.name.localeCompare(b.name));
      return n.files;
    };
    fill(root);
    return root;
  }

  let root = $derived(listing ? tree(listing.entries) : null);
  let byPath = $derived(new Map((listing?.entries ?? []).filter((e) => !e.dir).map((e) => [e.path, e])));
  let fileCount = $derived(byPath.size);

  type Row = { node: Node; depth: number };
  let rows = $derived.by((): Row[] => {
    if (!root) return [];
    const q = query.trim().toLowerCase();
    if (q) {
      return (listing?.entries ?? [])
        .filter((e) => !e.dir && e.path.toLowerCase().includes(q))
        .slice(0, MAX_ROWS)
        .map((e) => ({ node: { name: e.path, path: e.path, entry: e, children: [], files: pickable(e) ? [e.path] : [] }, depth: 0 }));
    }
    const out: Row[] = [];
    const walk = (n: Node, depth: number) => {
      for (const c of n.children) {
        if (out.length >= MAX_ROWS) return;
        out.push({ node: c, depth });
        if ((!c.entry || c.entry.dir) && open.has(c.path)) walk(c, depth + 1);
      }
    };
    walk(root, 0);
    return out;
  });

  $effect(() => {
    const list = [...picked].map((p) => byPath.get(p)).filter((e): e is ArchiveEntry => !!e);
    untrack(() => (selected = list));
  });

  let fetchTotal = $derived.by(() => {
    const blocks = new Set<number>();
    let sum = 0;
    for (const e of selected) {
      if (e.block !== null && e.block !== undefined) {
        if (blocks.has(e.block)) continue;
        blocks.add(e.block);
      }
      sum += e.fetch;
    }
    return sum;
  });

  function isFolder(n: Node) {
    return !n.entry || n.entry.dir;
  }

  function pickState(n: Node): 'all' | 'some' | 'none' {
    if (!n.files.length) return 'none';
    let have = 0;
    for (const f of n.files) if (picked.has(f)) have++;
    return have === 0 ? 'none' : have === n.files.length ? 'all' : 'some';
  }

  function toggle(n: Node) {
    const s = new Set(picked);
    const on = pickState(n) !== 'all';
    for (const f of n.files) on ? s.add(f) : s.delete(f);
    picked = s;
  }

  function toggleOpen(n: Node) {
    const s = new Set(open);
    s.has(n.path) ? s.delete(n.path) : s.add(n.path);
    open = s;
  }

  function all(on: boolean) {
    picked = on ? new Set([...byPath.values()].filter(pickable).map((e) => e.path)) : new Set();
  }

  function indeterminate(node: HTMLInputElement, value: boolean) {
    node.indeterminate = value;
    return { update: (v: boolean) => (node.indeterminate = v) };
  }

  function day(sec: number | null) {
    return sec ? new Date(sec * 1000).toLocaleDateString(i18n.lang) : '';
  }
</script>

<div class="rounded-[10px] border border-line bg-bg flex flex-col min-h-0">
  <div class="flex items-center gap-2 px-3 py-2 border-b border-line text-[12px]">
    <Icon name="archive" size={13} class="text-accent flex-shrink-0" />
    <span class="font-medium truncate flex-1">
      {#if listing}{FORMATS[listing.format] ?? listing.format} · {t('{count} files', { count: fileCount })} · {bytes(listing.size)}{:else}{t('Browse archive contents')}{/if}
    </span>
    <button type="button" class="text-[11px] text-accent hover:underline flex-shrink-0" onclick={onClose}>{t('Download whole file')}</button>
  </div>

  {#if loading}
    <p class="px-3 py-3 text-[11.5px] text-muted">{t('Reading the file list of the archive…')}</p>
  {:else if error}
    <div class="px-3 py-3 space-y-2">
      <p class="text-[12px] text-bad whitespace-pre-wrap">{error}</p>
      <button type="button" class="btn" onclick={load}><Icon name="refresh" size={12} />{t('Retry')}</button>
    </div>
  {:else if listing && root}
    <div class="px-3 pt-2 pb-1.5 border-b border-line space-y-1.5">
      <p class="text-[11px] text-muted">{t('Pick the files to download. Only their bytes are downloaded, not the whole archive.')}</p>
      <div class="flex items-center gap-2">
        <div class="relative flex-1">
          <Icon name="search" size={12} class="absolute left-2.5 top-1/2 -translate-y-1/2 text-dim" />
          <input class="field !pl-7" placeholder={t('Search in the archive')} bind:value={query} />
        </div>
        <button type="button" class="text-[11px] text-accent hover:underline" onclick={() => all(true)}>{t('Select all')}</button>
        <button type="button" class="text-[11px] text-accent hover:underline" onclick={() => all(false)}>{t('Select none')}</button>
      </div>
      {#if listing.truncated}
        <p class="text-[11px] text-warn flex items-start gap-1.5"><Icon name="alert" size={12} class="mt-[1px] flex-shrink-0" />{t('The list is incomplete: the archive has too many files, or reading on would mean downloading most of it.')}</p>
      {/if}
    </div>
    <div class="max-h-[260px] overflow-y-auto py-1" role="tree">
      {#each rows as { node, depth } (node.path)}
        {@const e = node.entry}
        {@const folder = isFolder(node)}
        {@const st = pickState(node)}
        {@const disabled = !node.files.length}
        <div class="flex items-center gap-1.5 pr-3 py-[3px] text-[11.5px] hover:bg-hover" style="padding-left: {12 + depth * 16}px" role="treeitem" aria-selected={st === 'all'}>
          {#if folder}
            <button type="button" class="icon-btn !w-[18px] !h-[18px] flex-shrink-0" aria-label={node.name} onclick={() => toggleOpen(node)}>
              <Icon name={open.has(node.path) ? 'chevronDown' : 'chevron'} size={10} />
            </button>
          {:else}
            <span class="w-[18px] flex-shrink-0"></span>
          {/if}
          <input type="checkbox" class="check flex-shrink-0" checked={st === 'all'} use:indeterminate={st === 'some'} {disabled} onchange={() => toggle(node)} aria-label={node.name} />
          <Icon name={folder ? (open.has(node.path) ? 'folderOpen' : 'folder') : 'file'} size={12} class="flex-shrink-0 {folder ? 'text-accent' : 'text-dim'}" />
          {#if folder}
            <button type="button" class="truncate flex-1 text-left" title={node.path} onclick={() => toggleOpen(node)}>{node.name}</button>
            <span class="text-dim flex-shrink-0">{node.files.length || ''}</span>
          {:else if e}
            <span class="truncate flex-1 {disabled ? 'text-dim' : ''}" title={node.path}>{node.name}</span>
            {#if e.problem}
              <span class="text-warn flex-shrink-0 flex items-center gap-1" title={t('This file cannot be taken out on its own. Download the whole archive to get it.')}><Icon name={e.problem === 'encrypted' ? 'lock' : 'alert'} size={11} />{t(PROBLEMS[e.problem] ?? 'Unsupported compression')}</span>
            {:else if e.shared > 1}
              <span class="text-muted flex-shrink-0" title={t('Solid block: the whole block ({size}) is downloaded for this file', { size: bytes(e.fetch) })}>{t('needs {size}', { size: bytes(e.fetch) })}</span>
            {/if}
            <span class="text-dim flex-shrink-0 w-[78px] text-right tabular-nums">{day(e.modified)}</span>
            <span class="text-muted flex-shrink-0 w-[64px] text-right tabular-nums">{bytes(e.size)}</span>
          {/if}
        </div>
      {:else}
        <p class="px-3 py-2 text-[11.5px] text-muted">{query.trim() ? t('No files match the search.') : t('The archive is empty.')}</p>
      {/each}
      {#if rows.length >= MAX_ROWS}
        <p class="px-3 py-1.5 text-[11px] text-dim">{t('Showing the first {count} entries. Search to find others.', { count: MAX_ROWS })}</p>
      {/if}
    </div>
    <div class="flex items-center justify-between gap-2 px-3 py-2 border-t border-line text-[11.5px]">
      <label class="flex items-center gap-2 cursor-pointer" title={t('Recreates the folders of the archive inside the download folder')}>
        <input type="checkbox" class="check" bind:checked={folders} />{t('Keep folders')}
      </label>
      <span class={selected.length ? 'text-white' : 'text-muted'}>{t('{count} selected · {size} to download', { count: selected.length, size: bytes(fetchTotal) })}</span>
    </div>
  {/if}
</div>
