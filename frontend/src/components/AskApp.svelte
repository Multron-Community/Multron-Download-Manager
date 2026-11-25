<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window';
  import { app } from '../lib/store.svelte';
  import { api, errorText } from '../lib/api';
  import { setLanguage, t } from '../lib/i18n.svelte';
  import { applyAppearance } from '../lib/theme';
  import { bytes, duration, percent, speed } from '../lib/format';
  import type { Download } from '../types';
  import { invoke } from '@tauri-apps/api/core';
  import { PROGRAM_FILE } from '../lib/files';
  import NewTaskModal from './NewTaskModal.svelte';
  import Icon from './Icon.svelte';

  const win = getCurrentWindow();
  let id = $state<string | null>(null);
  let item = $state<Download | null>(null);
  let gone = $state(false);
  let loading = $state(true);
  let tab = $state<'details' | 'speed' | 'finish'>('details');
  let limitOn = $state(false);
  let limitKb = $state(500);
  let limitApplied = $state(false);
  let openFileWhenDone = $state(false);
  let openFolderWhenDone = $state(false);
  let closeWhenDone = $state(false);
  let wasDone = false;

  const LIMIT_PRESETS = [64, 128, 256, 512, 1024, 2048, 5120];

  $effect(() => {
    if (item && !limitApplied) {
      limitOn = item.speedLimit > 0;
      if (item.speedLimit > 0) limitKb = item.speedLimit;
      limitApplied = true;
    }
  });

  $effect(() => {
    const now = item?.status === 'completed';
    if (now && !wasDone && item) {
      const d = item;
      if (openFolderWhenDone) api.showInFolder(d.id).catch(() => {});
      if (openFileWhenDone) openIt();
      if (closeWhenDone) setTimeout(close, 400);
    }
    wasDone = now;
  });

  async function applyLimit() {
    if (!item) return;
    const kib = limitOn ? Math.max(1, Math.round(Number(limitKb) || 0)) : 0;
    await act(() => api.setSpeedLimit(item!.id, kib));
    app.toast('success', kib ? t('Speed limited to {speed}', { speed: speed(kib * 1024) }) : t('Speed limit removed'));
  }

  async function setWhenDone(v: string) {
    if (!app.settings) return;
    await act(() => app.saveSettings({ ...app.settings!, whenDone: v as typeof app.settings.whenDone }));
  }

  let pct = $derived(item ? percent(item.downloaded, item.total) : 0);
  let done = $derived(item?.status === 'completed');
  let failed = $derived(item?.status === 'failed');
  let statusText = $derived.by(() => {
    if (!item) return '';
    if (item.status === 'active' && item.phase === 'waiting for host') return t('Waiting (the site asks to wait)');
    if (item.status === 'active' && item.phase === 'refreshing link') return t('Refreshing the link');
    return { queued: t('Queued'), active: t('Downloading'), paused: t('Paused'), completed: t('Completed'), failed: t('Failed') }[item.status];
  });

  function close() {
    win.close().catch(() => win.destroy());
  }

  async function started(newId: string) {
    id = newId;
    await api.askStarted().catch(() => {});
    item = (await api.downloads().catch(() => [])).find((d) => d.id === newId) ?? null;
    await win.setSize(new LogicalSize(560, 420)).catch(() => {});
  }

  async function act(fn: () => Promise<unknown>) {
    try {
      await fn();
    } catch (e) {
      app.toast('error', errorText(e));
    }
  }

  async function cancel() {
    if (!item) return close();
    if (!done) await act(() => api.remove(item!.id, true));
    close();
  }

  function askInOwnWindow(d: Download) {
    invoke('open_confirm_window', { id: d.id })
      .catch(() => invoke('notice_show_download', { id: d.id, start: true }))
      .catch(() => {})
      .finally(close);
  }

  function openIt() {
    const d = item;
    if (!d) return;
    if (app.disguised(d) || app.settings?.askBeforeOpeningFiles || (PROGRAM_FILE.test(d.name) && app.settings?.askBeforeStartingPrograms !== false)) {
      askInOwnWindow(d);
      return;
    }
    api.openFile(d.id).catch((e) => {
      const s = String(e);
      if (/confirm_start|blocked_threat/.test(s)) askInOwnWindow(d);
      else if (/rescan_needed|in_quarantine/.test(s)) invoke('notice_show_download', { id: d.id, start: true }).catch(() => {});
      else app.toast('error', s);
    });
  }

  onMount(() => {
    let stop: (() => void) | undefined;
    (async () => {
      const got = await api.askRequest().catch(() => null);
      const watch = got ? null : await api.askProgress().catch(() => null);
      if (!got && !watch) {
        loading = false;
        close();
        return;
      }
      const settings = got ? got[1] : watch![1];
      app.settings = settings;
      await setLanguage(settings.language);
      await applyAppearance(settings, true);
      if (document.documentElement.dataset.transparent !== 'true') {
        document.documentElement.style.background = 'var(--color-bg)';
        document.body.style.background = 'var(--color-bg)';
      }
      stop = await listen<Download[]>('downloads://changed', (e) => {
        if (!id) return;
        const next = e.payload.find((d) => d.id === id) ?? null;
        if (!next && item) gone = true;
        item = next;
      });
      loading = false;
      if (got) app.openAdd(got[0]);
      else await started(watch![0]);
    })();
    return () => stop?.();
  });
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && !id && close()} />

<div class="h-screen w-screen bg-bg text-white overflow-hidden">
  {#if loading}
    <div class="h-full flex items-center justify-center text-muted"><Icon name="refresh" size={18} class="animate-spin" /></div>
  {:else if !id}
    <NewTaskModal windowed onAdded={started} onClose={close} />
  {:else}
    <div class="h-full flex flex-col p-5 gap-4">
      <div class="flex items-start gap-3 min-w-0">
        <div class="w-[42px] h-[42px] rounded-[12px] flex items-center justify-center flex-shrink-0 {done ? 'bg-ok/15 text-ok status-pop' : failed ? 'bg-bad/15 text-bad status-pop' : 'file-badge'} {item?.status === 'active' ? 'is-active' : ''}">
          <Icon name={done ? 'check' : failed ? 'alert' : 'down'} size={20} />
        </div>
        <div class="min-w-0 flex-1">
          <div class="text-[14px] font-semibold truncate" title={item?.name}>{item?.name || t('Download file')}</div>
          <div class="text-[11px] text-dim truncate" title={item?.url}>{item?.url}</div>
        </div>
      </div>

      {#if item && (item.cloudState === 'threat' || item.cloudState === 'suspicious')}
        <div class="rounded-[10px] overflow-hidden border border-bad/50">
          <div class="px-3 py-2 flex items-center gap-2 text-[#fff] text-[12.5px] font-semibold" style="background: linear-gradient(135deg, #c62828, #ff5a4f)">
            <Icon name="shield" size={15} />{item.cloudState === 'threat' ? t('Threat found in a download') : t('Suspicious file')}
          </div>
          <div class="px-3 py-2 bg-bad/10 text-[12px] space-y-2">
            <div class="break-words">{item.cloudDetail || t('No details')}</div>
            <button class="btn btn-primary !h-[26px] !text-[11.5px]" style="background:#d32f2f;border-color:#d32f2f;color:#fff" onclick={() => invoke('notice_show_download', { id: item!.id, start: false, report: true }).catch(() => {})}><Icon name="alert" size={12} />{t('Show the threat')}</button>
          </div>
        </div>
      {/if}
      {#if gone}
        <p class="text-[12.5px] text-muted">{t('This download was removed from the list.')}</p>
      {:else if item}
        <div class="space-y-1.5">
          <div class="flex items-baseline justify-between text-[12px]">
            <span class={done ? 'text-ok' : failed ? 'text-bad' : 'text-muted'}>{statusText}</span>
            <span class="mono text-[18px] font-semibold">{item.total ? `${pct.toFixed(pct >= 100 ? 0 : 1)}%` : ''}</span>
          </div>
          <div class="progress-track w-full h-[8px] rounded-full overflow-hidden">
            <div
              class="h-full rounded-full transition-[width] duration-500 ease-out {done ? 'bg-ok' : failed ? 'bg-bad/70' : item.status === 'active' ? 'bg-accent progress-active' : 'bg-dim'} {item.status === 'active' && !item.total ? 'progress-live w-full' : ''}"
              style={item.total || done ? `width: ${done ? 100 : pct}%` : ''}
            ></div>
          </div>
        </div>

        <div class="flex items-center gap-1 border-b border-line -mb-1">
          {#each [['details', 'Details'], ['speed', 'Speed limiter'], ['finish', 'When finished']] as [k, label] (k)}
            <button type="button" class="px-3 h-[30px] text-[12px] border-b-2 -mb-px cursor-pointer transition-colors {tab === k ? 'border-accent text-white' : 'border-transparent text-muted hover:text-white'}" onclick={() => (tab = k as typeof tab)}>{t(label)}</button>
          {/each}
        </div>
        {#if tab === 'speed'}
          <div class="download-card rounded-[10px] border border-line px-4 py-3 space-y-3 text-[12px]">
            <label class="flex items-center gap-2 cursor-pointer"><input type="checkbox" class="toggle" bind:checked={limitOn} />{t('Limit the speed of this download')}</label>
            <div class="flex items-center gap-2 {limitOn ? '' : 'opacity-45 pointer-events-none'}">
              <span class="text-muted">{t('Maximum')}</span>
              <input type="number" min="1" class="field !h-[30px] !w-[110px] mono text-[12px]" bind:value={limitKb} />
              <span class="text-muted">KB/s</span>
              <span class="text-dim">≈ {speed((Number(limitKb) || 0) * 1024)}</span>
            </div>
            <div class="flex flex-wrap gap-1.5 {limitOn ? '' : 'opacity-45 pointer-events-none'}">
              {#each LIMIT_PRESETS as p (p)}
                <button type="button" class="h-[24px] px-2 rounded-[6px] border text-[11px] cursor-pointer {Number(limitKb) === p ? 'border-accent/60 bg-raised text-white' : 'border-line text-muted hover:text-white'}" onclick={() => (limitKb = p)}>{speed(p * 1024)}</button>
              {/each}
            </div>
            <div class="flex items-center justify-between gap-2">
              <span class="text-[11px] text-dim">{t('Only this download is limited. The global limit is in Settings → Transfer.')}</span>
              <button class="btn btn-accent !h-[28px]" onclick={applyLimit}>{t('Apply')}</button>
            </div>
          </div>
        {:else if tab === 'finish'}
          <div class="download-card rounded-[10px] border border-line px-4 py-3 space-y-2 text-[12px]">
            <label class="flex items-center gap-2 cursor-pointer"><input type="checkbox" class="check" bind:checked={openFileWhenDone} />{t('Open the file when this download finishes')}</label>
            <label class="flex items-center gap-2 cursor-pointer"><input type="checkbox" class="check" bind:checked={openFolderWhenDone} />{t('Open its folder when this download finishes')}</label>
            <label class="flex items-center gap-2 cursor-pointer"><input type="checkbox" class="check" bind:checked={closeWhenDone} />{t('Close this window when this download finishes')}</label>
            <div class="pt-2 mt-1 border-t border-line/60 flex items-center gap-2">
              <span class="text-muted">{t('When all downloads finish')}</span>
              <select class="field !h-[30px] !w-auto text-[12px]" value={app.settings?.whenDone ?? 'nothing'} onchange={(e) => setWhenDone(e.currentTarget.value)}>
                <option value="nothing">{t('Do nothing')}</option>
                <option value="sleep">{t('Sleep')}</option>
                <option value="shutdown">{t('Shut down')}</option>
                <option value="quit">{t('Quit Multron Download Manager')}</option>
              </select>
            </div>
            {#if done}<p class="text-[11px] text-dim">{t('This download has already finished.')}</p>{/if}
          </div>
        {:else}
        <div class="download-card rounded-[10px] border border-line px-4 py-3 grid grid-cols-2 gap-x-6 gap-y-2 text-[12px]">
          <div class="flex justify-between gap-2"><span class="text-muted">{t('Size')}</span><span class="mono">{bytes(item.downloaded)}{item.total ? ` / ${bytes(item.total)}` : ''}</span></div>
          <div class="flex justify-between gap-2"><span class="text-muted">{t('Speed')}</span><span class="mono">{item.status === 'active' ? speed(item.speed) : '—'}</span></div>
          <div class="flex justify-between gap-2"><span class="text-muted">{t('Time left')}</span><span class="mono">{item.status === 'active' && item.eta != null ? duration(item.eta) : '—'}</span></div>
          <div class="flex justify-between gap-2"><span class="text-muted">{t('Connections')}</span><span class="mono">{item.status === 'active' ? item.connections : '—'}</span></div>
          <div class="col-span-2 flex justify-between gap-3 min-w-0"><span class="text-muted flex-shrink-0">{t('Save to')}</span><span class="truncate" title={item.dir}>{item.dir}</span></div>
          {#if failed && item.error}
            <div class="col-span-2 text-bad text-[11.5px] break-words">{item.error}</div>
          {/if}
        </div>
        {/if}
      {/if}

      <div class="flex-1"></div>
      <div class="flex items-center gap-2">
        {#if item && done}
          <button class="btn" onclick={() => act(() => api.showInFolder(item!.id))}><Icon name="folder" size={13} />{t('Show in folder')}</button>
          <div class="flex-1"></div>
          <button class="btn" onclick={close}>{t('Close')}</button>
          <button class="btn btn-primary" onclick={openIt}>{t('Open')}</button>
        {:else if item}
          {#if item.status === 'paused' || failed}
            <button class="btn" onclick={() => act(() => api.resume(item!.id))}><Icon name="play" size={12} />{failed ? t('Retry') : t('Resume')}</button>
          {:else}
            <button class="btn" onclick={() => act(() => api.pause(item!.id))}><Icon name="pause" size={12} />{t('Pause')}</button>
          {/if}
          <button class="btn hover:!text-bad" onclick={cancel}>{t('Cancel')}</button>
          <div class="flex-1"></div>
          <button class="btn" onclick={close} title={t('The download goes on in the main window.')}>{t('Hide')}</button>
        {:else}
          <div class="flex-1"></div>
          <button class="btn" onclick={close}>{t('Close')}</button>
        {/if}
      </div>
    </div>
  {/if}

  <div class="fixed bottom-3 left-3 right-3 flex flex-col gap-2 pointer-events-none">
    {#each app.toasts as toast (toast.id)}
      <div class="toast-in pointer-events-auto border border-line rounded-[12px] px-3 py-2 text-[12px] {toast.kind === 'error' ? 'text-bad' : ''}">{toast.message}</div>
    {/each}
  </div>
</div>
