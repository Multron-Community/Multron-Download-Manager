<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { listen } from '@tauri-apps/api/event';
  import { app } from './lib/store.svelte';
  import { t } from './lib/i18n.svelte';
  import { api, errorText } from './lib/api';
  import { applyAppearance } from './lib/theme';
  import { builtinBackground } from './lib/backgrounds';
  import { prepareWallpaper } from './lib/wallpaper';
  import { bytes, speed } from './lib/format';
  import Sidebar from './components/Sidebar.svelte';
  import CommandPalette from './components/CommandPalette.svelte';
  import Icon from './components/Icon.svelte';
  import Header from './components/Header.svelte';
  import DownloadList from './components/DownloadList.svelte';
  import DetailsPane from './components/DetailsPane.svelte';
  import StatusBar from './components/StatusBar.svelte';
  import NewTaskModal from './components/NewTaskModal.svelte';
  import Dialogs from './components/Dialogs.svelte';
  import ExtensionSetup from './components/ExtensionSetup.svelte';
  import BatchModal from './components/BatchModal.svelte';
  import CreateTorrentModal from './components/CreateTorrentModal.svelte';
  import type { Component } from 'svelte';

  const VIEWS: Record<string, () => Promise<{ default: Component }>> = {
    settings: () => import('./components/SettingsView.svelte'),
    rss: () => import('./components/RssView.svelte'),
    search: () => import('./components/SearchView.svelte'),
    grabber: () => import('./components/LinkGrabberView.svelte'),
    stats: () => import('./components/StatsView.svelte'),
    history: () => import('./components/HistoryView.svelte')
  };

  let initError = $state<string | null>(null);

  $effect(() => {
    const a = app.appearancePreview ?? app.settings;
    if (a && app.ready) {
      applyAppearance({
        theme: a.theme,
        accentColor: a.accentColor,
        accentCustom: a.accentCustom,
        accentContrast: a.accentContrast,
        accentText: a.accentText,
        buttonColor: a.buttonColor,
        glassEffect: a.glassEffect,
        hoverGlow: a.hoverGlow,
        clickRipple: a.clickRipple,
        stateGlow: a.stateGlow,
        motion: a.motion,
        effectsOff: a.effectsOff,
        headingColor: a.headingColor,
        lightBrightness: a.lightBrightness,
        darkBrightness: a.darkBrightness,
        roundedCorners: a.roundedCorners,
        transparency: a.transparency,
        backdropMaterial: a.backdropMaterial,
        transparencyLevel: a.transparencyLevel,
        wallpaper: !!wallpaperUrl,
        wallpaperOpacity: a.backgroundOpacity
      });
    }
  });

  let wallpaperSource = $state<string | null>(null);
  let wallpaperUrl = $state<string | null>(null);
  let wallpaperFor = '';
  $effect(() => {
    const path = (app.appearancePreview ?? app.settings)?.backgroundImage ?? '';
    if (path === wallpaperFor) return;
    wallpaperFor = path;
    if (!path) {
      wallpaperSource = null;
      return;
    }
    const builtin = builtinBackground(path);
    if (builtin) {
      wallpaperSource = builtin.image;
      return;
    }
    api.backgroundData(path).then((url) => {
      if (wallpaperFor === path) wallpaperSource = url;
    }).catch(() => (wallpaperSource = null));
  });
  let wallpaperBlur = $derived(Math.max(0, (app.appearancePreview ?? app.settings)?.backgroundBlur ?? 0));
  let viewSize = $state(0);
  let wallpaperJob = 0;
  function showWallpaper(url: string | null) {
    if (wallpaperUrl?.startsWith('blob:') && wallpaperUrl !== url) URL.revokeObjectURL(wallpaperUrl);
    wallpaperUrl = url;
  }
  $effect(() => {
    const src = wallpaperSource;
    const blur = wallpaperBlur;
    if (blur > 0) void viewSize;
    const job = ++wallpaperJob;
    if (!src) {
      showWallpaper(null);
      return;
    }
    const timer = setTimeout(() => {
      prepareWallpaper(src, blur)
        .then((url) => {
          if (job === wallpaperJob) showWallpaper(url);
          else if (url.startsWith('blob:')) URL.revokeObjectURL(url);
        })
        .catch(() => job === wallpaperJob && showWallpaper(src));
    }, untrack(() => wallpaperUrl) ? 150 : 0);
    return () => clearTimeout(timer);
  });
  let dragging = $state(false);

  onMount(() => {
    app.init().catch((e) => (initError = errorText(e)));

    let unlisten: (() => void) | undefined;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        const p = event.payload;
        if (p.type === 'enter' || p.type === 'over') dragging = true;
        else if (p.type === 'leave') dragging = false;
        else if (p.type === 'drop') {
          dragging = false;
          const torrent = p.paths.find((x) => x.toLowerCase().endsWith('.torrent'));
          if (torrent) app.openAdd({ url: '', kind: 'torrent' }, torrent);
        }
      })
      .then((fn) => (unlisten = fn))
      .catch(() => {});

    const IDLE_MS = 10000;
    let windowShown = true;
    let idle = false;
    let idleTimer = 0;
    let sweep = 0;
    const loops = () => document.getAnimations().filter((a) => a.effect?.getTiming().iterations === Infinity);
    const pauseLoops = () => {
      for (const a of loops()) if (a.playState === 'running') a.pause();
    };
    const markHidden = () => {
      const paused = !windowShown || idle || document.visibilityState === 'hidden';
      if (paused === !!sweep) return;
      if (paused) {
        pauseLoops();
        sweep = window.setInterval(pauseLoops, 2000);
      } else {
        clearInterval(sweep);
        sweep = 0;
        for (const a of loops()) if (a.playState === 'paused') a.play();
      }
    };
    const wake = () => {
      clearTimeout(idleTimer);
      idleTimer = window.setTimeout(() => {
        idle = true;
        markHidden();
      }, IDLE_MS);
      if (idle) {
        idle = false;
        markHidden();
      }
    };
    const onFocus = () => wake();
    const onBlur = () => {
      clearTimeout(idleTimer);
      idle = true;
      markHidden();
    };
    const activity = ['pointermove', 'pointerdown', 'keydown', 'wheel'] as const;
    for (const type of activity) window.addEventListener(type, wake, { passive: true, capture: true });
    window.addEventListener('focus', onFocus);
    window.addEventListener('blur', onBlur);
    wake();
    let unlistenVisibility: (() => void) | undefined;
    listen<boolean>('main-visibility', (e) => {
      windowShown = e.payload;
      markHidden();
    })
      .then((fn) => (unlistenVisibility = fn))
      .catch(() => {});
    const win = getCurrentWindow();
    Promise.all([win.isVisible(), win.isMinimized()])
      .then(([visible, minimized]) => {
        windowShown = visible && !minimized;
        markHidden();
      })
      .catch(() => {});
    document.addEventListener('visibilitychange', markHidden);

    const onPaste = (e: ClipboardEvent) => {
      const target = e.target as HTMLElement | null;
      if (target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA')) return;
      const text = e.clipboardData?.getData('text')?.trim();
      if (text && /^(https?:\/\/|magnet:)/i.test(text)) app.openAdd({ url: text });
    };
    window.addEventListener('paste', onPaste);
    let resizeTimer = 0;
    const onResize = () => {
      clearTimeout(resizeTimer);
      resizeTimer = window.setTimeout(() => (viewSize = window.innerWidth * 10000 + window.innerHeight), 400);
    };
    window.addEventListener('resize', onResize);
    return () => {
      clearTimeout(resizeTimer);
      window.removeEventListener('resize', onResize);
      unlisten?.();
      unlistenVisibility?.();
      document.removeEventListener('visibilitychange', markHidden);
      clearTimeout(idleTimer);
      clearInterval(sweep);
      for (const type of activity) window.removeEventListener(type, wake, { capture: true });
      window.removeEventListener('focus', onFocus);
      window.removeEventListener('blur', onBlur);
      window.removeEventListener('paste', onPaste);
    };
  });

  function retryUpdate() {
    if (app.mandatoryUpdate) app.mandatoryUpdate = { ...app.mandatoryUpdate, phase: 'downloading', error: null };
    api.retryMandatoryUpdate().catch(() => {});
  }
</script>

{#if wallpaperUrl}
  <div class="fixed inset-0 -z-10 bg-cover bg-center pointer-events-none" style="background-image: url({wallpaperUrl}); transform: translateZ(0);"></div>
{/if}
<div class="app-root w-screen h-screen bg-bg flex flex-col overflow-hidden {app.stats.downloadSpeed > 0 || app.stats.uploadSpeed > 0 ? 'busy' : ''}">
  {#if initError}
    <div class="m-auto text-center space-y-2">
      <p class="text-bad text-[14px]">Multron Download Manager could not start</p>
      <p class="text-muted text-[12px] mono">{initError}</p>
    </div>
  {:else if app.ready}
    <div class="app-shell flex-1 flex overflow-hidden min-h-0">
      <Sidebar />
      {#if VIEWS[app.view]}
        {#key app.view}
          {#await VIEWS[app.view]()}
            <div class="flex-1 bg-bg"></div>
          {:then view}
            <div class="view-enter flex-1 flex min-w-0 min-h-0 overflow-hidden">
              <view.default />
            </div>
          {/await}
        {/key}
      {:else}
        <div class="flex-1 flex flex-col min-w-0 min-h-0">
          <Header />
          <DownloadList />
          {#if app.selected}
            <DetailsPane item={app.selected} />
          {/if}
        </div>
      {/if}
    </div>
    <StatusBar />
  {/if}
</div>

{#if dragging}
  <div class="fixed inset-0 z-40 bg-accent-strong/15 border-2 border-dashed border-accent m-2 rounded-[14px] flex items-center justify-center pointer-events-none">
    <span class="text-[15px] font-semibold">Drop .torrent files to add them</span>
  </div>
{/if}

<NewTaskModal
  onAdded={(id) => {
    const done = app.addDone;
    app.addDone = null;
    done?.(id);
  }}
/>
<ExtensionSetup />
<BatchModal />
<CreateTorrentModal />
<Dialogs />

{#if app.mandatoryUpdate}
  {@const u = app.mandatoryUpdate}
  {@const p = app.appUpdateProgress}
  {@const pct = p?.total ? Math.min(100, Math.round((p.downloaded / p.total) * 100)) : null}
  <div class="fixed inset-0 z-[80] bg-bg flex items-center justify-center p-6" role="alertdialog" aria-live="polite">
    <div class="w-full max-w-[420px] min-h-[250px] bg-menu border border-line rounded-[16px] shadow-2xl p-6 flex flex-col items-center justify-center text-center gap-4">
      <img src="/logo.png" alt="" class="w-16 h-16 rounded-full" />
      {#if u.phase === 'failed'}
        <h2 class="text-[16px] font-semibold">{t('Multron Download Manager could not update')}</h2>
        <p class="text-[12.5px] text-muted leading-relaxed">{t('The update will be tried again the next time Multron Download Manager starts. You can keep using this version until then.')}</p>
        {#if u.error}<p class="text-[11.5px] text-bad break-words">{u.error}</p>{/if}
        <div class="flex gap-2">
          <button class="btn" onclick={() => (app.mandatoryUpdate = null)}>{t('Continue')}</button>
          <button class="btn btn-accent" onclick={retryUpdate}><Icon name="refresh" size={13} />{t('Retry')}</button>
        </div>
      {:else}
        <h2 class="text-[16px] font-semibold">{t('Updating Multron Download Manager to {version}', { version: u.version })}</h2>
        <p class="text-[12.5px] text-muted leading-[18px] min-h-[36px] flex items-center">{u.phase === 'installing' ? t('Installing the update. Multron Download Manager restarts by itself.') : t('Downloading the update. This only takes a moment.')}</p>
        <div class="w-full h-[6px] bg-line rounded-full overflow-hidden">
          <div class="h-full bg-accent rounded-full transition-[width] duration-300 {pct === null || u.phase === 'installing' ? 'progress-live w-full' : ''}" style={pct !== null && u.phase !== 'installing' ? `width: ${pct}%` : ''}></div>
        </div>
        <p class="text-[11.5px] text-dim mono tabular-nums h-[16px] leading-[16px] w-full truncate">{u.phase === 'installing' || !p ? '' : (pct !== null ? pct + '% · ' : '') + bytes(p.downloaded) + (p.total ? ' / ' + bytes(p.total) : '') + ' · ' + speed(p.speed)}{u.error ? ' · ' + t('Retrying…') : ''}</p>
      {/if}
    </div>
  </div>
{/if}
<CommandPalette />
