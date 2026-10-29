<script lang="ts">
  import { onMount } from 'svelte';
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import { app } from './lib/store.svelte';
  import { t } from './lib/i18n.svelte';
  import { api, errorText } from './lib/api';
  import { applyAppearance } from './lib/theme';
  import { bytes, speed } from './lib/format';
  import Sidebar from './components/Sidebar.svelte';
  import Icon from './components/Icon.svelte';
  import Header from './components/Header.svelte';
  import DownloadList from './components/DownloadList.svelte';
  import DetailsPane from './components/DetailsPane.svelte';
  import StatusBar from './components/StatusBar.svelte';
  import NewTaskModal from './components/NewTaskModal.svelte';
  import SettingsView from './components/SettingsView.svelte';
  import Dialogs from './components/Dialogs.svelte';
  import ExtensionSetup from './components/ExtensionSetup.svelte';
  import BatchModal from './components/BatchModal.svelte';
  import RssView from './components/RssView.svelte';
  import CreateTorrentModal from './components/CreateTorrentModal.svelte';

  let initError = $state<string | null>(null);

  $effect(() => {
    const a = app.appearancePreview ?? app.settings;
    if (a && app.ready) {
      applyAppearance({
        theme: a.theme,
        accentColor: a.accentColor,
        transparency: a.transparency,
        backdropMaterial: a.backdropMaterial,
        transparencyLevel: a.transparencyLevel
      });
    }
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

    const onPaste = (e: ClipboardEvent) => {
      const target = e.target as HTMLElement | null;
      if (target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA')) return;
      const text = e.clipboardData?.getData('text')?.trim();
      if (text && /^(https?:\/\/|magnet:)/i.test(text)) app.openAdd({ url: text });
    };
    window.addEventListener('paste', onPaste);
    return () => {
      unlisten?.();
      window.removeEventListener('paste', onPaste);
    };
  });

  function retryUpdate() {
    if (app.mandatoryUpdate) app.mandatoryUpdate = { ...app.mandatoryUpdate, phase: 'downloading', error: null };
    api.retryMandatoryUpdate().catch(() => {});
  }
</script>

<div class="w-screen h-screen bg-bg flex flex-col overflow-hidden">
  {#if initError}
    <div class="m-auto text-center space-y-2">
      <p class="text-bad text-[14px]">Multron Download Manager could not start</p>
      <p class="text-muted text-[12px] mono">{initError}</p>
    </div>
  {:else if app.ready}
    <div class="flex-1 flex overflow-hidden min-h-0">
      <Sidebar />
      {#if app.view === 'settings'}
        <SettingsView />
      {:else if app.view === 'rss'}
        <RssView />
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

<NewTaskModal />
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
