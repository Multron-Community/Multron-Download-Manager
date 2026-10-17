<script lang="ts">
  import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog';
  import { app } from '../lib/store.svelte';
  import { api, errorText } from '../lib/api';
  import { t } from '../lib/i18n.svelte';
  import Icon from './Icon.svelte';

  const PIECES: [number, string][] = [
    [0, 'Automatic'],
    [256, '256 KiB'],
    [512, '512 KiB'],
    [1024, '1 MiB'],
    [2048, '2 MiB'],
    [4096, '4 MiB'],
    [8192, '8 MiB'],
    [16384, '16 MiB']
  ];

  let source = $state('');
  let trackers = $state('');
  let piece = $state(0);
  let saveTo = $state('');
  let seed = $state(true);
  let busy = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    if (!app.createTorrentOpen) return;
    source = '';
    saveTo = '';
    error = null;
    trackers = (app.settings?.extraTrackers ?? []).join('\n');
  });

  const baseName = (p: string) => p.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || 'torrent';

  async function pick(folder: boolean) {
    const res = await openDialog({ directory: folder, multiple: false });
    if (typeof res === 'string') {
      source = res;
      if (!saveTo) {
        const dir = app.settings?.downloadDir ?? '';
        const sep = app.platform === 'windows' ? '\\' : '/';
        saveTo = `${dir.replace(/[\\/]+$/, '')}${sep}${baseName(res)}.torrent`;
      }
    }
  }

  async function pickSave() {
    const res = await saveDialog({ defaultPath: saveTo || `${baseName(source)}.torrent`, filters: [{ name: 'Torrent', extensions: ['torrent'] }] });
    if (res) saveTo = res;
  }

  async function create() {
    if (!source || !saveTo) return;
    busy = true;
    error = null;
    try {
      await api.createTorrent(source, trackers.split('\n'), piece || null, saveTo, seed);
      app.toast('success', seed ? 'Torrent created and seeding' : 'Torrent created');
      app.createTorrentOpen = false;
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }
</script>

{#if app.createTorrentOpen}
  <div class="fixed inset-0 bg-black/60 backdrop-blur-sm z-[55] flex items-center justify-center p-4" role="presentation">
    <div class="w-full max-w-[620px] bg-menu border border-line rounded-[14px] shadow-2xl p-5 space-y-4">
      <div class="flex items-center gap-3">
        <div class="w-10 h-10 rounded-full bg-accent-strong/25 text-accent flex items-center justify-center"><Icon name="magnet" size={18} /></div>
        <h2 class="text-[15px] font-semibold flex-1">{t('Create torrent')}</h2>
        <button class="icon-btn" title={t('Close')} onclick={() => (app.createTorrentOpen = false)}><Icon name="x" size={14} /></button>
      </div>

      <label class="flex flex-col gap-1">
        <span class="label">{t('File or folder to share')}</span>
        <div class="flex gap-2">
          <input class="field" bind:value={source} />
          <button class="btn" onclick={() => pick(false)}><Icon name="file" size={13} />{t('File')}</button>
          <button class="btn" onclick={() => pick(true)}><Icon name="folder" size={13} />{t('Folder')}</button>
        </div>
      </label>

      <label class="flex flex-col gap-1">
        <span class="label">{t('Trackers (one per line)')}</span>
        <textarea class="field" rows="4" placeholder="udp://tracker.opentrackr.org:1337/announce" bind:value={trackers}></textarea>
      </label>

      <div class="flex gap-4 flex-wrap items-end">
        <label class="flex flex-col gap-1">
          <span class="label">{t('Piece size')}</span>
          <select class="field !w-[160px]" bind:value={piece}>
            {#each PIECES as [v, l]}<option value={v}>{t(l)}</option>{/each}
          </select>
        </label>
        <label class="flex flex-col gap-1 flex-1 min-w-[260px]">
          <span class="label">{t('Save .torrent as')}</span>
          <div class="flex gap-2">
            <input class="field" bind:value={saveTo} />
            <button class="btn" onclick={pickSave}><Icon name="folderOpen" size={13} /></button>
          </div>
        </label>
      </div>

      <label class="flex items-center gap-2 text-[12.5px] cursor-pointer">
        <input type="checkbox" class="check" bind:checked={seed} />{t('Start seeding right away')}
      </label>

      {#if error}<p class="text-[12px] text-bad break-all">{error}</p>{/if}

      <div class="flex justify-end gap-2">
        <button class="btn" onclick={() => (app.createTorrentOpen = false)}>{t('Cancel')}</button>
        <button class="btn btn-accent" disabled={busy || !source || !saveTo} onclick={create}>
          <Icon name="magnet" size={13} />{busy ? t('Hashing files…') : t('Create torrent')}
        </button>
      </div>
    </div>
  </div>
{/if}
