<script lang="ts">
  import { QUALITIES } from '../lib/qualities';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { app } from '../lib/store.svelte';
  import { api, errorText } from '../lib/api';
  import { t } from '../lib/i18n.svelte';
  import type { BatchItem } from '../types';
  import Icon from './Icon.svelte';

  type Filter = 'all' | 'video' | 'image' | 'audio' | 'file' | 'post';
  const FILTERS: { id: Filter; label: string; icon: string }[] = [
    { id: 'all', label: 'All', icon: 'list' },
    { id: 'video', label: 'Videos', icon: 'film' },
    { id: 'image', label: 'Images', icon: 'image' },
    { id: 'audio', label: 'Audio', icon: 'music' },
    { id: 'file', label: 'Files', icon: 'file' },
    { id: 'post', label: 'Posts', icon: 'link' }
  ];

  let filter = $state<Filter>('all');
  let search = $state('');
  let selected = $state<Set<string>>(new Set());
  let hideKnown = $state(true);
  let quality = $state('best');
  let dir = $state('');
  let subfolder = $state(true);
  let paused = $state(false);
  let busy = $state(false);
  let progress = $state(0);

  let batch = $derived(app.batch);
  let known = $derived(new Set(app.downloads.map((d) => d.url)));
  let items = $derived((batch?.items ?? []).filter((i) => !(hideKnown && known.has(i.url))));
  let counts = $derived.by(() => {
    const c: Record<string, number> = { all: items.length };
    for (const i of items) c[i.mediaType] = (c[i.mediaType] ?? 0) + 1;
    return c;
  });
  let visible = $derived.by(() => {
    const q = search.trim().toLowerCase();
    return items.filter(
      (i) =>
        (filter === 'all' || i.mediaType === filter) &&
        (!q || (i.name ?? '').toLowerCase().includes(q) || i.url.toLowerCase().includes(q))
    );
  });
  let chosen = $derived(items.filter((i) => selected.has(i.url)));
  let hasMedia = $derived(chosen.some((i) => i.kind === 'media'));

  $effect(() => {
    const b = app.batch;
    if (!b) return;
    filter = 'all';
    search = '';
    dir = app.settings?.downloadDir ?? '';
    const media = b.items.filter((i) => i.mediaType === 'video' || i.mediaType === 'post');
    selected = new Set((media.length ? media : b.items).filter((i) => !known.has(i.url)).map((i) => i.url));
  });

  function toggle(url: string) {
    const next = new Set(selected);
    if (next.has(url)) next.delete(url);
    else next.add(url);
    selected = next;
  }

  function selectVisible(on: boolean) {
    const next = new Set(selected);
    for (const i of visible) {
      if (on) next.add(i.url);
      else next.delete(i.url);
    }
    selected = next;
  }

  function invertVisible() {
    const next = new Set(selected);
    for (const i of visible) {
      if (next.has(i.url)) next.delete(i.url);
      else next.add(i.url);
    }
    selected = next;
  }

  const host = (url: string) => {
    try {
      return new URL(url).hostname.replace(/^www\./, '');
    } catch {
      return url.startsWith('magnet:') ? 'magnet' : '';
    }
  };

  const ext = (url: string) => {
    try {
      const m = new URL(url).pathname.match(/\.([a-z0-9]{2,5})$/i);
      return m ? m[1].toUpperCase() : '';
    } catch {
      return '';
    }
  };

  const ICONS: Record<string, string> = { video: 'film', image: 'image', audio: 'music', file: 'file', post: 'link' };

  function folderName(title: string) {
    return title.replace(/[<>:"/\\|?*\u0000-\u001f]/g, '_').replace(/\s+/g, ' ').trim().replace(/[. ]+$/, '').slice(0, 80) || 'Multron Download Manager';
  }

  function targetDir() {
    if (!subfolder || !batch) return dir || null;
    const sep = app.platform === 'windows' ? '\\' : '/';
    return `${dir.replace(/[\\/]+$/, '')}${sep}${folderName(batch.pageTitle || host(batch.pageUrl))}`;
  }

  async function browse() {
    const res = await openDialog({ directory: true, defaultPath: dir || undefined });
    if (typeof res === 'string') dir = res;
  }

  async function start() {
    if (!batch || !chosen.length) return;
    busy = true;
    progress = 0;
    const target = targetDir();
    let failed = 0;
    for (const item of chosen as BatchItem[]) {
      try {
        await api.add({
          url: item.url,
          kind: item.kind,
          quality: item.kind === 'media' ? quality : null,
          dir: target,
          filename: item.filename ?? null,
          pageTitle: item.name ?? null,
          thumbnail: item.thumbnail ?? null,
          referrer: item.referrer ?? batch.pageUrl,
          userAgent: batch.userAgent ?? null,
          cookies: batch.cookies ?? [],
          startPaused: paused
        });
      } catch (e) {
        failed++;
        console.warn('batch add failed', item.url, errorText(e));
      }
      progress++;
    }
    busy = false;
    const added = chosen.length - failed;
    app.toast(failed ? 'warning' : 'success', failed ? t('{added} added, {failed} failed', { added, failed }) : t('{count} downloads added', { count: added }));
    app.batch = null;
  }
</script>

{#if batch}
  <div class="fixed inset-0 bg-black/60 backdrop-blur-sm z-[55] flex items-center justify-center p-4" role="presentation">
    <div class="w-full max-w-[860px] h-[86vh] bg-menu border border-line rounded-[14px] shadow-2xl flex flex-col">
      <div class="p-5 pb-3 flex items-start gap-3">
        <div class="w-10 h-10 rounded-full bg-accent-strong/25 text-accent flex items-center justify-center flex-shrink-0">
          <Icon name="down" size={18} />
        </div>
        <div class="flex-1 min-w-0">
          <h2 class="text-[15px] font-semibold">{t('Download from this page')}</h2>
          <p class="text-[12px] text-muted truncate" title={batch.pageUrl}>{batch.pageTitle || batch.pageUrl}</p>
        </div>
        <button class="icon-btn" title={t('Close')} onclick={() => (app.batch = null)}><Icon name="x" size={14} /></button>
      </div>

      <div class="px-5 flex flex-wrap items-center gap-2">
        {#each FILTERS as f}
          {#if f.id === 'all' || counts[f.id]}
            <button
              type="button"
              class="h-[28px] px-3 rounded-full border text-[12px] flex items-center gap-1.5 cursor-pointer {filter === f.id ? 'border-accent/60 bg-raised text-white' : 'border-line text-muted hover:text-white hover:bg-hover'}"
              onclick={() => (filter = f.id)}
            >
              <Icon name={f.icon} size={12} />{t(f.label)} <span class="text-dim">{counts[f.id] ?? 0}</span>
            </button>
          {/if}
        {/each}
        <div class="flex-1"></div>
        <div class="w-[220px] h-[28px] px-[10px] bg-bg border border-line rounded-[8px] flex items-center gap-[8px]">
          <Icon name="search" size={12} class="text-muted flex-shrink-0" />
          <input class="w-full bg-transparent text-[12px] outline-none" placeholder={t('Filter by name or address')} bind:value={search} />
        </div>
      </div>

      <div class="px-5 pt-2 pb-1 flex items-center gap-3 text-[11.5px] text-muted">
        <button class="hover:text-white cursor-pointer" onclick={() => selectVisible(true)}>{t('Select all')}</button>
        <button class="hover:text-white cursor-pointer" onclick={() => selectVisible(false)}>{t('Select none')}</button>
        <button class="hover:text-white cursor-pointer" onclick={invertVisible}>{t('Invert')}</button>
        <label class="flex items-center gap-1.5 cursor-pointer ml-auto">
          <input type="checkbox" class="check" bind:checked={hideKnown} />{t('Hide items already in Multron Download Manager')}
        </label>
      </div>

      <div class="flex-1 min-h-0 overflow-y-auto mx-5 border border-line rounded-[10px]">
        {#each visible as item (item.url)}
          <label class="flex items-center gap-3 px-3 py-2 border-b border-line/60 last:border-b-0 cursor-pointer hover:bg-hover">
            <input type="checkbox" class="check" checked={selected.has(item.url)} onchange={() => toggle(item.url)} />
            <div class="w-[52px] h-[34px] rounded-[6px] bg-line flex items-center justify-center overflow-hidden flex-shrink-0 text-muted">
              {#if item.thumbnail || item.mediaType === 'image'}
                <img src={item.thumbnail || item.url} alt="" loading="lazy" class="w-full h-full object-cover" />
              {:else}
                <Icon name={ICONS[item.mediaType] ?? 'file'} size={15} />
              {/if}
            </div>
            <div class="min-w-0 flex-1">
              <div class="text-[12.5px] truncate" title={item.name ?? item.url}>{item.name || item.url}</div>
              <div class="text-[11px] text-dim truncate" title={item.url}>{host(item.url)}{ext(item.url) ? ` · ${ext(item.url)}` : ''}{item.kind === 'media' ? ` · ${t('via yt-dlp')}` : ''}</div>
            </div>
            <span class="text-[10.5px] px-2 py-[2px] rounded-full bg-line text-muted">{t(FILTERS.find((f) => f.id === item.mediaType)?.label ?? 'Files')}</span>
          </label>
        {:else}
          <p class="p-6 text-center text-[12px] text-muted">{t('Nothing matches this filter.')}</p>
        {/each}
      </div>

      <div class="p-5 pt-3 space-y-3">
        <div class="flex flex-wrap items-end gap-3">
          <label class="flex flex-col gap-1 flex-1 min-w-[260px]">
            <span class="label">{t('Save to')}</span>
            <div class="flex gap-2">
              <input class="field" bind:value={dir} />
              <button class="btn" onclick={browse}><Icon name="folderOpen" size={13} />{t('Browse')}</button>
            </div>
          </label>
          {#if hasMedia}
            <label class="flex flex-col gap-1">
              <span class="label">{t('Video quality')}</span>
              <select class="field !w-[170px]" bind:value={quality}>
                {#each QUALITIES as q}<option value={q.id}>{t(q.label)}</option>{/each}
              </select>
            </label>
          {/if}
        </div>
        <div class="flex flex-wrap items-center gap-4 text-[12px]">
          <label class="flex items-center gap-1.5 cursor-pointer"><input type="checkbox" class="check" bind:checked={subfolder} />{t('Put them in a folder named after the page')}</label>
          <label class="flex items-center gap-1.5 cursor-pointer"><input type="checkbox" class="check" bind:checked={paused} />{t('Add paused')}</label>
          <div class="flex-1"></div>
          <button class="btn" onclick={() => (app.batch = null)}>{t('Cancel')}</button>
          <button class="btn btn-accent" disabled={busy || !chosen.length} onclick={start}>
            <Icon name="down" size={13} />
            {busy ? t('Adding {done}/{total}…', { done: progress, total: chosen.length }) : t('Download {count} selected', { count: chosen.length })}
          </button>
        </div>
        {#if app.settings && !app.settings.sequential && chosen.length > 3}
          <p class="text-[11px] text-dim">{t('Tip: the “One at a time” button at the top downloads them one after another.')}</p>
        {/if}
      </div>
    </div>
  </div>
{/if}
