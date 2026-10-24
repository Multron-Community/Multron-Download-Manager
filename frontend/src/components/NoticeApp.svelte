<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { setLanguage, t } from '../lib/i18n.svelte';
  import { applyAppearance } from '../lib/theme';
  import { PROGRAM_FILE } from '../lib/files';
  import type { Settings } from '../types';
  import Icon from './Icon.svelte';

  interface Notice {
    id: number;
    kind: string;
    title: string;
    body: string;
    download: string | null;
    tone: 'accent' | 'ok' | 'warn' | 'bad';
  }

  let notices = $state<Notice[]>([]);
  let box = $state<HTMLDivElement | null>(null);
  const timers = new Map<number, ReturnType<typeof setTimeout>>();
  const hovered = new Set<number>();

  function lifetime(n: Notice) {
    if (n.tone === 'bad') return 20000;
    return n.kind === 'started' ? 6000 : 9000;
  }

  function schedule(n: Notice) {
    if (timers.has(n.id) || hovered.has(n.id)) return;
    timers.set(n.id, setTimeout(() => dismiss(n.id), lifetime(n)));
  }

  async function refresh() {
    notices = await invoke<Notice[]>('notice_list');
    for (const n of notices) schedule(n);
    for (const id of [...timers.keys()]) {
      if (!notices.some((n) => n.id === id)) {
        clearTimeout(timers.get(id));
        timers.delete(id);
      }
    }
    await tick();
    await layout();
  }

  async function layout() {
    const height = notices.length && box ? Math.ceil(Math.max(box.scrollHeight, box.getBoundingClientRect().height)) : 0;
    await invoke('notice_layout', { height }).catch(() => {});
    requestAnimationFrame(() => {
      const again = notices.length && box ? Math.ceil(Math.max(box.scrollHeight, box.getBoundingClientRect().height)) : 0;
      if (again !== height) invoke('notice_layout', { height: again }).catch(() => {});
    });
  }

  async function dismiss(id: number) {
    clearTimeout(timers.get(id));
    timers.delete(id);
    await invoke('notice_dismiss', { id }).catch(() => {});
    await refresh();
  }

  function hold(n: Notice, on: boolean) {
    if (on) {
      hovered.add(n.id);
      clearTimeout(timers.get(n.id));
      timers.delete(n.id);
    } else {
      hovered.delete(n.id);
      schedule(n);
    }
  }

  async function show(n: Notice, start = false) {
    await invoke('notice_show_download', { id: n.download, start }).catch(() => {});
    dismiss(n.id);
  }

  async function open(n: Notice) {
    if (!n.download) return;
    if (PROGRAM_FILE.test(n.body)) {
      await show(n, true);
      return;
    }
    const asked = await invoke('open_file', { id: n.download }).then(
      () => false,
      (e) => String(e).includes('confirm_start')
    );
    if (asked) await show(n, true);
    else dismiss(n.id);
  }

  async function folder(n: Notice) {
    if (!n.download) return;
    await invoke('show_in_folder', { id: n.download }).catch(() => {});
    dismiss(n.id);
  }

  const icon: Record<string, string> = { started: 'down', done: 'check', failed: 'alert', threat: 'alert', hash_bad: 'alert', hash_ok: 'shield', scan_cloud: 'shield' };

  onMount(() => {
    document.documentElement.style.background = 'var(--color-raised)';
    document.body.style.background = 'var(--color-raised)';
    let stop: (() => void) | undefined;
    (async () => {
      const settings = await invoke<Settings>('get_settings').catch(() => null);
      if (settings) {
        await setLanguage(settings.language);
        await applyAppearance(settings, false);
        document.documentElement.style.setProperty('--color-window', 'var(--color-raised)');
      }
      stop = await listen('notice://changed', () => refresh());
      await refresh();
    })();
    const observer = new ResizeObserver(() => layout());
    if (box) observer.observe(box);
    return () => {
      stop?.();
      observer.disconnect();
    };
  });
</script>

<div bind:this={box} class="flex flex-col bg-raised divide-y divide-line border border-line rounded-[8px] overflow-hidden">
  {#each notices as n (n.id)}
    <div
      role="presentation"
      class="relative bg-raised hover:bg-hover pl-4 pr-3 py-3 flex gap-3 items-start cursor-pointer transition-colors"
      onmouseenter={() => hold(n, true)}
      onmouseleave={() => hold(n, false)}
      onclick={() => show(n)}
    >
      <div class="absolute left-0 top-0 bottom-0 w-[4px] {n.tone === 'accent' ? 'bg-accent' : n.tone === 'ok' ? 'bg-ok' : n.tone === 'warn' ? 'bg-warn' : 'bg-bad'}"></div>
      <div class="w-[34px] h-[34px] rounded-full flex items-center justify-center flex-shrink-0 {n.tone === 'accent' ? 'bg-accent/20 text-accent' : n.tone === 'ok' ? 'bg-ok/20 text-ok' : n.tone === 'warn' ? 'bg-warn/20 text-warn' : 'bg-bad/20 text-bad'}">
        <Icon name={icon[n.kind] ?? 'info'} size={17} />
      </div>
      <div class="min-w-0 flex-1">
        <div class="text-[11px] text-muted">Multron Download Manager</div>
        <div class="text-[13px] font-semibold">{n.title}</div>
        <div class="text-[12px] text-muted line-clamp-2 break-words" title={n.body}>{n.body}</div>
        {#if n.kind === 'done' && n.download}
          <div class="flex gap-1.5 mt-2">
            <button class="btn !h-[26px] !text-[11.5px]" onclick={(e) => { e.stopPropagation(); open(n); }}>{t('Open')}</button>
            <button class="btn !h-[26px] !text-[11.5px]" onclick={(e) => { e.stopPropagation(); folder(n); }}><Icon name="folderOpen" size={12} />{t('Show in folder')}</button>
          </div>
        {/if}
      </div>
      <button class="icon-btn !w-[22px] !h-[22px]" title={t('Close')} onclick={(e) => { e.stopPropagation(); dismiss(n.id); }}><Icon name="x" size={12} /></button>
    </div>
  {/each}
</div>
