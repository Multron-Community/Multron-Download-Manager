<script lang="ts">
  import { app } from '../lib/store.svelte';
  import { api } from '../lib/api';
  import { t } from '../lib/i18n.svelte';
  import DownloadItem from './DownloadItem.svelte';
  import Icon from './Icon.svelte';

  function typing(e: KeyboardEvent) {
    const el = e.target as HTMLElement | null;
    return !!el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.isContentEditable);
  }

  function onKey(e: KeyboardEvent) {
    if (app.view !== 'downloads' || typing(e) || app.removeTargets.length || app.addOpen) return;
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'a') {
      e.preventDefault();
      app.checkAll();
    } else if (e.key === 'Delete') {
      const items = app.checkedItems.length ? app.checkedItems : app.selected ? [app.selected] : [];
      if (items.length) {
        e.preventDefault();
        app.askRemove(items);
      }
    } else if (e.key === 'Escape' && app.checked.length) {
      app.clearChecks();
    }
  }

  async function each(fn: (id: string) => Promise<void>) {
    const ids = [...app.checked];
    await app.run(async () => {
      for (const id of ids) await fn(id).catch(() => {});
    });
  }
</script>

<svelte:window onkeydown={onKey} />

<main class="flex-1 p-4 overflow-y-auto flex flex-col gap-[6px] bg-bg min-h-0">
  {#if app.checked.length}
    <div class="sticky top-0 z-10 -mt-1 mb-1 flex items-center gap-2 px-3 py-2 rounded-[10px] border border-accent/40 bg-menu shadow-lg text-[12.5px]">
      <span class="font-medium">{t('{count} selected', { count: app.checked.length })}</span>
      <button class="btn !h-[26px]" onclick={() => app.checkAll()}>{t('Select all')}</button>
      <div class="flex-1"></div>
      <button class="btn !h-[26px]" onclick={() => each((id) => api.pause(id))}><Icon name="pause" size={12} />{t('Pause')}</button>
      <button class="btn !h-[26px]" onclick={() => each((id) => api.resume(id))}><Icon name="play" size={12} />{t('Resume')}</button>
      <button class="btn btn-danger !h-[26px]" onclick={() => app.askRemove(app.checkedItems)}><Icon name="trash" size={12} />{t('Remove')}</button>
      <button class="icon-btn !w-7 !h-7" title={t('Clear selection')} onclick={() => app.clearChecks()}><Icon name="x" size={13} /></button>
    </div>
  {/if}
  {#if app.filtered.length > 0}
    {#each app.filtered as item (item.id)}
      <DownloadItem {item} />
    {/each}
  {:else}
    <div class="flex-1 flex flex-col items-center justify-center min-h-[300px] text-center p-8 border border-dashed border-line/70 rounded-[12px] bg-subtle/20">
      <div class="w-14 h-14 rounded-full bg-line/60 flex items-center justify-center text-muted mb-4">
        <Icon name="down" size={26} stroke={1.6} />
      </div>
      <h3 class="text-[15px] font-semibold mb-1.5">{t('No downloads yet')}</h3>
      <p class="text-[12.5px] text-muted max-w-sm mb-5 leading-relaxed">
        {t('Paste a link, drop a .torrent file or send downloads from your browser with the Multron Download Manager extension.')}
      </p>
      <button type="button" onclick={() => app.openAdd()} class="btn btn-primary">{t('Add download')}</button>
    </div>
  {/if}
</main>
