<script lang="ts">
  import { tick } from 'svelte';
  import { app } from '../lib/store.svelte';
  import { api } from '../lib/api';
  import { t } from '../lib/i18n.svelte';
  import Icon from './Icon.svelte';

  type Item = { id: string; label: string; hint?: string; icon: string; keys?: string; run: () => void };

  let query = $state('');
  let index = $state(0);
  let input = $state<HTMLInputElement>();

  const go = (section: string) => () => {
    app.settingsSection = section;
    app.view = 'settings';
  };
  const view = (v: typeof app.view) => () => (app.view = v);

  let actions = $derived<Item[]>([
    { id: 'new', label: t('New download'), icon: 'plus', keys: 'Ctrl+N', run: () => app.openAdd() },
    { id: 'pause', label: t('Pause all'), icon: 'pause', keys: 'Ctrl+Shift+P', run: () => app.run(api.pauseAll) },
    { id: 'resume', label: t('Resume all'), icon: 'play', keys: 'Ctrl+Shift+R', run: () => app.run(api.resumeAll) },
    { id: 'stop', label: t('Stop all (also stops seeding torrents)'), icon: 'stop', run: () => app.run(api.stopAll) },
    { id: 'downloads', label: t('All Downloads'), icon: 'list', run: () => { app.view = 'downloads'; app.statusFilter = 'all'; app.categoryFilter = 'all'; app.kindFilter = 'all'; app.tagFilter = null; } },
    { id: 'grabber', label: 'LinkGrabber', icon: 'link', keys: 'Ctrl+L', run: view('grabber') },
    { id: 'search', label: t('Torrent search'), icon: 'search', run: view('search') },
    { id: 'rss', label: t('RSS Feeds'), icon: 'rss', run: view('rss') },
    { id: 'stats', label: t('Statistics'), icon: 'gauge', run: view('stats') },
    { id: 'history', label: t('Download history'), icon: 'clock', keys: 'Ctrl+H', run: view('history') },
    { id: 'create', label: t('Create torrent'), icon: 'magnet', run: () => (app.createTorrentOpen = true) },
    { id: 'category', label: t('New category'), icon: 'folder', run: () => (app.categoryDialog = { index: null }) },
    { id: 'theme', label: t('Switch between light and dark theme'), icon: 'moon', run: () => app.toggleTheme() },
    { id: 'turtle', label: t('Turn alternative speed limits on or off'), icon: 'turtle', run: () => app.toggleAltSpeed() },
    { id: 'scanall', label: t('Scan all'), icon: 'shield', run: () => app.scanAllInCloud() },
    { id: 's-settings', label: t('Settings'), icon: 'settings', keys: 'Ctrl+,', run: go('general') },
    ...[
      ['appearance', 'Appearance', 'palette'],
      ['language', 'Language', 'globe'],
      ['files', 'Files', 'folder'],
      ['connection', 'Connection', 'link'],
      ['transfer', 'Transfer', 'down'],
      ['browser', 'Browser Integration', 'globe'],
      ['media', 'Media (yt-dlp)', 'film'],
      ['torrent', 'Torrent', 'magnet'],
      ['scheduler', 'Scheduler', 'clock'],
      ['logins', 'Site logins', 'users'],
      ['sitelimits', 'Per-site limits', 'gauge'],
      ['siterules', 'Site rules', 'code'],
      ['rules', 'Rename & move rules', 'edit'],
      ['automation', 'When Downloads Finish', 'power'],
      ['cloud', 'Multron Cloud', 'cloud'],
      ['anonymity', 'Privacy', 'eye'],
      ['virustotal', 'VirusTotal', 'plus'],
      ['captcha', 'CAPTCHAs', 'shield'],
      ['security', 'Security', 'lock'],
      ['backup', 'Backup & restore', 'archive'],
      ['about', 'Updates & About', 'info']
    ].map(([id, label, icon]) => ({ id: 's-' + id, label: t(label), hint: t('Settings'), icon, run: go(id) }))
  ]);

  let results = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const acts = q ? actions.filter((a) => a.label.toLowerCase().includes(q) || (a.hint ?? '').toLowerCase().includes(q)) : actions;
    const dls: Item[] = q
      ? app.downloads
          .filter((d) => d.name.toLowerCase().includes(q) || (d.tags ?? []).some((x) => x.toLowerCase().includes(q)))
          .slice(0, 8)
          .map((d) => ({
            id: 'd-' + d.id,
            label: d.name,
            hint: t('Download'),
            icon: d.kind === 'torrent' ? 'magnet' : d.kind === 'media' ? 'film' : 'file',
            run: () => {
              app.view = 'downloads';
              app.statusFilter = 'all';
              app.categoryFilter = 'all';
              app.kindFilter = 'all';
              app.tagFilter = null;
              app.selectedId = d.id;
            }
          }))
      : [];
    return [...acts, ...dls].slice(0, 40);
  });

  $effect(() => {
    if (app.paletteOpen) {
      query = '';
      index = 0;
      tick().then(() => input?.focus());
    }
  });

  $effect(() => {
    if (index >= results.length) index = Math.max(0, results.length - 1);
  });

  function choose(item: Item | undefined) {
    if (!item) return;
    app.paletteOpen = false;
    item.run();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      index = Math.min(results.length - 1, index + 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      index = Math.max(0, index - 1);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      choose(results[index]);
    } else if (e.key === 'Escape') {
      app.paletteOpen = false;
    }
  }

  function typing(e: KeyboardEvent) {
    const el = e.target as HTMLElement | null;
    return !!el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.tagName === 'SELECT' || el.isContentEditable);
  }

  function shortcuts(e: KeyboardEvent) {
    if (app.settings?.shortcutsEnabled === false) return;
    const ctrl = e.ctrlKey || e.metaKey;
    const k = e.key.toLowerCase();
    if (ctrl && (k === 'k' || (k === 'p' && !e.shiftKey))) {
      e.preventDefault();
      app.paletteOpen = !app.paletteOpen;
      return;
    }
    if (app.paletteOpen) return;
    if (ctrl && k === 'n') {
      e.preventDefault();
      app.openAdd();
    } else if (ctrl && k === ',') {
      e.preventDefault();
      go('general')();
    } else if (ctrl && k === 'f') {
      e.preventDefault();
      app.view = 'downloads';
      tick().then(() => (document.getElementById('global-search') as HTMLInputElement | null)?.focus());
    } else if (ctrl && e.shiftKey && k === 'p') {
      e.preventDefault();
      app.run(api.pauseAll);
    } else if (ctrl && e.shiftKey && k === 'r') {
      e.preventDefault();
      app.run(api.resumeAll);
    } else if (ctrl && k === 'l') {
      e.preventDefault();
      app.view = 'grabber';
    } else if (ctrl && k === 'h') {
      e.preventDefault();
      app.view = 'history';
    } else if (!ctrl && !typing(e) && app.view === 'downloads') {
      const targets = app.checkedItems.length ? app.checkedItems : app.selected ? [app.selected] : [];
      if (e.key === 'Delete' && targets.length) {
        e.preventDefault();
        app.askRemove(targets);
      } else if (e.key === ' ' && targets.length) {
        e.preventDefault();
        for (const d of targets) {
          if (d.status === 'active' || d.status === 'queued') app.run(() => api.pause(d.id));
          else if (d.status === 'paused' || d.status === 'failed') app.run(() => api.resume(d.id));
        }
      }
    }
  }
</script>

<svelte:window onkeydown={shortcuts} />

{#if app.paletteOpen}
  <div class="fixed inset-0 bg-black/50 backdrop-blur-sm z-[60] flex items-start justify-center pt-[12vh] px-4" role="presentation" onclick={(e) => e.target === e.currentTarget && (app.paletteOpen = false)}>
    <div class="w-full max-w-[580px] bg-menu border border-line rounded-[14px] shadow-2xl overflow-hidden">
      <div class="flex items-center gap-2.5 px-4 h-[50px] border-b border-line">
        <Icon name="search" size={15} class="text-muted" />
        <input bind:this={input} class="flex-1 bg-transparent outline-none text-[14px]" placeholder={t('Type a command, a setting or a download…')} bind:value={query} onkeydown={onKey} />
        <kbd class="text-[10.5px] text-dim border border-line rounded px-1.5 py-[1px]">Esc</kbd>
      </div>
      <div class="max-h-[52vh] overflow-y-auto py-1.5">
        {#each results as r, i (r.id)}
          <button type="button" class="w-full px-4 h-[36px] flex items-center gap-3 text-left text-[12.5px] {i === index ? 'bg-accent/15 text-white' : 'text-muted hover:bg-hover'}" onmouseenter={() => (index = i)} onclick={() => choose(r)}>
            <Icon name={r.icon} size={14} class={i === index ? 'text-accent' : ''} />
            <span class="flex-1 truncate">{r.label}</span>
            {#if r.hint}<span class="text-[11px] text-dim">{r.hint}</span>{/if}
            {#if r.keys}<kbd class="text-[10.5px] text-dim border border-line rounded px-1.5 py-[1px]">{r.keys}</kbd>{/if}
          </button>
        {:else}
          <p class="px-4 py-6 text-center text-[12px] text-muted">{t('Nothing found.')}</p>
        {/each}
      </div>
      <div class="px-4 py-2 border-t border-line text-[10.5px] text-dim flex gap-4">
        <span>↑ ↓ {t('to move')}</span><span>Enter {t('to open')}</span><span>Ctrl+K {t('opens this anywhere')}</span>
      </div>
    </div>
  </div>
{/if}
