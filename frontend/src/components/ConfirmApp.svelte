<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { app } from '../lib/store.svelte';
  import { api, errorText } from '../lib/api';
  import { setLanguage } from '../lib/i18n.svelte';
  import { applyAppearance } from '../lib/theme';
  import type { Download } from '../types';
  import StartConfirm from './StartConfirm.svelte';
  import Icon from './Icon.svelte';

  const win = getCurrentWindow();
  let item = $state<Download | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);

  function close() {
    win.close().catch(() => win.destroy());
  }

  async function confirm() {
    if (!item || busy) return;
    busy = true;
    error = null;
    try {
      await api.openFile(item.id, true);
      close();
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  async function scanFirst() {
    if (!item || busy) return;
    busy = true;
    error = null;
    try {
      await api.cloudScan(item.id);
      await invoke('notice_show_download', { id: item.id, start: false, report: true });
      close();
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  onMount(() => {
    let stop: (() => void) | undefined;
    (async () => {
      const id = await invoke<string | null>('ask_confirm').catch(() => null);
      const boot = id ? await api.bootstrap().catch(() => null) : null;
      item = boot?.downloads.find((d) => d.id === id) ?? null;
      if (!boot || !item) return close();
      app.settings = boot.settings;
      app.platform = boot.platform;
      await setLanguage(boot.settings.language);
      await applyAppearance(boot.settings, true);
      if (document.documentElement.dataset.transparent !== 'true') {
        document.documentElement.style.background = 'var(--color-bg)';
        document.body.style.background = 'var(--color-bg)';
      }
      stop = await listen<Download[]>('downloads://changed', (e) => {
        const next = e.payload.find((d) => d.id === id);
        if (next) item = next;
      });
    })();
    return () => stop?.();
  });
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && close()} />

<div class="h-screen w-screen bg-bg text-white overflow-hidden flex flex-col">
  {#if item}
    {#if error}
      <div class="mx-5 mt-4 rounded-[9px] border border-bad/40 bg-bad/10 px-3 py-2 text-[12px] text-bad break-words">{error}</div>
    {/if}
    <div class="flex-1 min-h-0">
      <StartConfirm d={item} windowed onCancel={close} onConfirm={confirm} onScanFirst={scanFirst} />
    </div>
  {:else}
    <div class="h-full flex items-center justify-center text-muted"><Icon name="refresh" size={18} class="animate-spin" /></div>
  {/if}
</div>
