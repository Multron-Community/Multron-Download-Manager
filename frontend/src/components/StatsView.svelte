<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from '../lib/store.svelte';
  import { api } from '../lib/api';
  import { t } from '../lib/i18n.svelte';
  import { bytes } from '../lib/format';
  import type { HistoryItem } from '../types';
  import Icon from './Icon.svelte';

  let history = $state<HistoryItem[]>([]);
  let range = $state<7 | 30 | 90>(30);

  onMount(() => {
    api.downloadHistory().then((h) => (history = h)).catch(() => {});
  });

  const DAY = 24 * 3600 * 1000;
  const host = (u: string) => {
    try {
      return new URL(u).hostname.replace(/^www\./, '');
    } catch {
      return u.startsWith('magnet:') ? 'magnet' : '—';
    }
  };

  let done = $derived.by(() => {
    const map = new Map<string, { id: string; url: string; size: number; at: number; kind: string }>();
    for (const h of history) map.set(h.id, { id: h.id, url: h.url, size: h.size ?? 0, at: h.at, kind: h.kind });
    for (const d of app.downloads) {
      if (d.status === 'completed' && d.completedAt) map.set(d.id, { id: d.id, url: d.url, size: d.total ?? d.downloaded, at: d.completedAt, kind: d.kind });
    }
    return [...map.values()];
  });

  function sum(from: number) {
    const list = done.filter((d) => d.at >= from);
    return { count: list.length, bytes: list.reduce((a, d) => a + d.size, 0) };
  }

  let today = $derived(sum(new Date(new Date().toDateString()).getTime()));
  let week = $derived(sum(Date.now() - 7 * DAY));
  let month = $derived(sum(Date.now() - 30 * DAY));
  let all = $derived(sum(0));

  let days = $derived.by(() => {
    const start = new Date(new Date().toDateString()).getTime() - (range - 1) * DAY;
    const out = Array.from({ length: range }, (_, i) => ({ at: start + i * DAY, bytes: 0, count: 0 }));
    for (const d of done) {
      const i = Math.floor((d.at - start) / DAY);
      if (i >= 0 && i < range) {
        out[i].bytes += d.size;
        out[i].count += 1;
      }
    }
    return out;
  });
  let maxDay = $derived(Math.max(1, ...days.map((d) => d.bytes)));

  let sites = $derived.by(() => {
    const m = new Map<string, { bytes: number; count: number }>();
    for (const d of done) {
      const h = host(d.url);
      const e = m.get(h) ?? { bytes: 0, count: 0 };
      e.bytes += d.size;
      e.count += 1;
      m.set(h, e);
    }
    return [...m.entries()].sort((a, b) => b[1].bytes - a[1].bytes).slice(0, 8);
  });
  let maxSite = $derived(Math.max(1, ...sites.map(([, s]) => s.bytes)));

  let kinds = $derived.by(() => {
    const m: Record<string, number> = {};
    for (const d of done) m[d.kind] = (m[d.kind] ?? 0) + d.size;
    return m;
  });

  let categories = $derived.by(() => {
    const m = new Map<string, number>();
    for (const d of app.downloads) {
      if (d.status !== 'completed') continue;
      const c = app.categoryOf(d);
      m.set(c, (m.get(c) ?? 0) + (d.total ?? d.downloaded));
    }
    return [...m.entries()].sort((a, b) => b[1] - a[1]);
  });
  let catTotal = $derived(Math.max(1, categories.reduce((a, [, v]) => a + v, 0)));
  const catLabel = (c: string) => (c.startsWith('custom:') ? c.slice(7) : t(c.charAt(0).toUpperCase() + c.slice(1)));
  const HUES = [205, 160, 35, 280, 340, 95, 15, 250];
</script>

<div class="flex-1 overflow-y-auto min-w-0 bg-bg">
  <div class="max-w-[1100px] mx-auto p-6 space-y-5">
    <div class="flex items-center gap-2">
      <Icon name="gauge" size={18} class="text-accent" />
      <h1 class="text-[17px] font-semibold flex-1">{t('Statistics')}</h1>
    </div>

    <div class="grid grid-cols-2 lg:grid-cols-4 gap-3">
      {#each [[t('Today'), today], [t('Last 7 days'), week], [t('Last 30 days'), month], [t('All time'), all]] as [label, v]}
        {@const s = v as { count: number; bytes: number }}
        <div class="download-card rounded-[12px] border border-line p-4">
          <div class="text-[11.5px] text-muted">{label}</div>
          <div class="text-[22px] font-semibold mt-1">{bytes(s.bytes)}</div>
          <div class="text-[11.5px] text-dim">{t('{count} downloads', { count: s.count })}</div>
        </div>
      {/each}
    </div>

    <div class="download-card rounded-[12px] border border-line p-4">
      <div class="flex items-center gap-2 mb-3">
        <span class="text-[13px] font-semibold flex-1">{t('Downloaded per day')}</span>
        {#each [7, 30, 90] as r}
          <button class="chip {range === r ? 'chip-on' : ''}" onclick={() => (range = r as 7 | 30 | 90)}>{t('{count} days', { count: r })}</button>
        {/each}
      </div>
      <div class="stat-grid h-[160px] flex items-end gap-[3px] relative">
        {#if maxDay > 1}<span class="absolute top-0 left-0 -translate-y-[2px] text-[10px] text-dim pointer-events-none">{bytes(maxDay)}</span>{/if}
        {#each days as d, i (d.at)}
          <div class="flex-1 h-full flex flex-col justify-end group relative" title={`${new Date(d.at).toLocaleDateString()} · ${bytes(d.bytes)} · ${d.count}`}>
            <div class="stat-bar w-full rounded-t-[3px]" style="height: {d.bytes ? `${Math.max(3, (d.bytes / maxDay) * 100)}%` : '3px'}; animation-delay: {Math.min(i * 12, 600)}ms; {d.bytes ? '' : 'opacity: 0.22'}"></div>
          </div>
        {/each}
      </div>
      <div class="flex justify-between text-[10.5px] text-dim mt-1.5">
        <span>{new Date(days[0]?.at ?? Date.now()).toLocaleDateString()}</span>
        <span>{t('Today')}</span>
      </div>
    </div>

    <div class="grid grid-cols-1 lg:grid-cols-2 gap-3">
      <div class="download-card rounded-[12px] border border-line p-4">
        <div class="text-[13px] font-semibold mb-3">{t('Most used sites')}</div>
        {#each sites as [h, s] (h)}
          <div class="mb-2.5">
            <div class="flex justify-between text-[12px] mb-1"><span class="truncate">{h}</span><span class="text-muted flex-shrink-0 ml-2">{bytes(s.bytes)} · {s.count}</span></div>
            <div class="h-[5px] bg-line rounded-full overflow-hidden"><div class="h-full bg-accent rounded-full" style="width: {(s.bytes / maxSite) * 100}%"></div></div>
          </div>
        {:else}
          <p class="text-[12px] text-muted">{t('Nothing downloaded yet.')}</p>
        {/each}
      </div>

      <div class="download-card rounded-[12px] border border-line p-4">
        <div class="text-[13px] font-semibold mb-3">{t('Space by category')}</div>
        {#if categories.length}
          <div class="h-[12px] rounded-full overflow-hidden flex mb-3">
            {#each categories as [c, v], i (c)}
              <div style="width: {(v / catTotal) * 100}%; background: hsl({HUES[i % HUES.length]} 65% 55%)" title={`${catLabel(c)} · ${bytes(v)}`}></div>
            {/each}
          </div>
          <div class="grid grid-cols-2 gap-x-4 gap-y-1.5">
            {#each categories as [c, v], i (c)}
              <div class="flex items-center gap-2 text-[12px] min-w-0"><span class="w-[9px] h-[9px] rounded-full flex-shrink-0" style="background: hsl({HUES[i % HUES.length]} 65% 55%)"></span><span class="truncate flex-1">{catLabel(c)}</span><span class="text-muted">{bytes(v)}</span></div>
            {/each}
          </div>
        {:else}
          <p class="text-[12px] text-muted">{t('Nothing downloaded yet.')}</p>
        {/if}
        <div class="flex gap-4 mt-4 text-[11.5px] text-muted">
          <span>{t('Direct links')}: <b class="text-white font-medium">{bytes(kinds.http ?? 0)}</b></span>
          <span>{t('Media')}: <b class="text-white font-medium">{bytes(kinds.media ?? 0)}</b></span>
          <span>{t('Torrents')}: <b class="text-white font-medium">{bytes(kinds.torrent ?? 0)}</b></span>
        </div>
      </div>
    </div>
  </div>
</div>
