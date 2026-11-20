<script lang="ts">
  import { open as openDialog, ask } from '@tauri-apps/plugin-dialog';
  import { app } from '../lib/store.svelte';
  import { t } from '../lib/i18n.svelte';
  import type { CustomCategory } from '../types';
  import Icon from './Icon.svelte';

  const ICONS = ['folder', 'film', 'music', 'image', 'doc', 'book', 'archive', 'app', 'code', 'game', 'cube', 'font', 'subtitle', 'disc', 'camera', 'palette', 'briefcase', 'star', 'heart', 'globe', 'magnet', 'shield', 'puzzle', 'file'];
  const PRESETS: { name: string; icon: string; exts: string[] }[] = [
    { name: 'E-books', icon: 'book', exts: ['epub', 'mobi', 'azw3', 'fb2', 'djvu', 'cbz', 'cbr'] },
    { name: 'Subtitles', icon: 'subtitle', exts: ['srt', 'ass', 'ssa', 'vtt', 'sub', 'idx'] },
    { name: 'Fonts', icon: 'font', exts: ['ttf', 'otf', 'woff', 'woff2'] },
    { name: '3D models', icon: 'cube', exts: ['stl', 'obj', 'fbx', 'blend', 'gltf', 'glb', '3mf'] },
    { name: 'Design files', icon: 'palette', exts: ['psd', 'ai', 'sketch', 'fig', 'xd', 'afdesign'] },
    { name: 'Disk images', icon: 'disc', exts: ['iso', 'img', 'vhd', 'vhdx', 'vmdk'] },
    { name: 'Source code', icon: 'code', exts: ['py', 'js', 'ts', 'rs', 'go', 'java', 'c', 'cpp', 'cs'] },
    { name: 'Games', icon: 'game', exts: ['nsp', 'xci', 'cia', 'pkg', 'wbfs', 'rom'] }
  ];

  let index = $derived(app.categoryDialog?.index ?? null);
  let editing = $derived(index !== null);
  let name = $state('');
  let icon = $state('folder');
  let exts = $state<string[]>([]);
  let extInput = $state('');
  let folder = $state('');
  let ownFolder = $state(false);
  let busy = $state(false);

  $effect(() => {
    const d = app.categoryDialog;
    if (!d) return;
    const c = d.index !== null ? app.settings?.customCategories[d.index] : undefined;
    name = c?.name ?? '';
    icon = c?.icon || 'folder';
    exts = [...(c?.extensions ?? [])];
    folder = c?.folder ?? '';
    ownFolder = !!c?.folder;
    extInput = '';
  });

  let others = $derived((app.settings?.customCategories ?? []).filter((_, i) => i !== index));
  let taken = $derived(exts.filter((e) => others.some((o) => o.extensions.includes(e))));
  let nameTaken = $derived(others.some((o) => o.name.toLowerCase() === name.trim().toLowerCase()));
  let canSave = $derived(!!name.trim() && exts.length > 0 && !nameTaken && (!ownFolder || !!folder.trim()) && !busy);
  let sep = $derived(app.platform === 'windows' ? '\\' : '/');
  let target = $derived(ownFolder ? folder || '…' : `${(app.settings?.downloadDir ?? '').replace(/[\\/]+$/, '')}${sep}${name.trim() || t('Name')}`);

  function addExts(text: string) {
    const next = [...exts];
    for (const raw of text.split(/[\s,;]+/)) {
      const e = raw.trim().replace(/^\*?\./, '').toLowerCase();
      if (e && /^[a-z0-9_-]{1,12}$/.test(e) && !next.includes(e)) next.push(e);
    }
    exts = next;
  }

  function onExtKey(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ' ' || e.key === ',') {
      e.preventDefault();
      addExts(extInput);
      extInput = '';
    } else if (e.key === 'Backspace' && !extInput && exts.length) {
      exts = exts.slice(0, -1);
    }
  }

  function usePreset(p: (typeof PRESETS)[number]) {
    if (!name.trim()) name = t(p.name);
    icon = p.icon;
    addExts(p.exts.join(' '));
  }

  async function browse() {
    const res = await openDialog({ directory: true, defaultPath: folder || app.settings?.downloadDir || undefined });
    if (typeof res === 'string') folder = res;
  }

  function close() {
    app.categoryDialog = null;
  }

  async function save() {
    const s = app.settings;
    if (!s || !canSave) return;
    if (extInput.trim()) {
      addExts(extInput);
      extInput = '';
    }
    busy = true;
    const entry: CustomCategory = { name: name.trim(), extensions: exts, folder: ownFolder ? folder.trim() : '', icon };
    const list = [...s.customCategories];
    if (index !== null) {
      const old = list[index];
      list[index] = entry;
      if (old && app.categoryFilter === `custom:${old.name}`) app.categoryFilter = `custom:${entry.name}`;
    } else list.push(entry);
    const ok = await app.saveSettings({ ...s, customCategories: list });
    busy = false;
    if (ok) close();
  }

  async function remove() {
    const s = app.settings;
    if (!s || index === null) return;
    const c = s.customCategories[index];
    const yes = await ask(t('Remove the category {name}? Downloaded files are not touched.', { name: c.name }), { title: t('Remove category'), kind: 'warning', okLabel: t('Remove'), cancelLabel: t('Cancel') }).catch(() => false);
    if (!yes) return;
    if (app.categoryFilter === `custom:${c.name}`) app.categoryFilter = 'all';
    if (await app.saveSettings({ ...s, customCategories: s.customCategories.filter((_, i) => i !== index) })) close();
  }
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && app.categoryDialog && close()} />

{#if app.categoryDialog}
  <div class="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4" role="presentation" onclick={(e) => e.target === e.currentTarget && close()}>
    <form class="w-full max-w-[640px] max-h-[92vh] bg-menu border border-line rounded-[14px] shadow-2xl flex flex-col" onsubmit={(e) => { e.preventDefault(); save(); }}>
      <div class="flex items-center gap-3 px-5 pt-4 pb-3 border-b border-line">
        <div class="w-[38px] h-[38px] rounded-[10px] bg-accent/15 text-accent flex items-center justify-center flex-shrink-0"><Icon name={icon} size={18} /></div>
        <div class="flex-1 min-w-0">
          <h2 class="text-[15px] font-semibold">{editing ? t('Edit category') : t('New category')}</h2>
          <p class="text-[11.5px] text-muted">{t('Downloads with these file types are listed under this category and saved in its folder.')}</p>
        </div>
        <button type="button" class="icon-btn" aria-label={t('Close')} onclick={close}><Icon name="x" size={14} /></button>
      </div>

      <div class="overflow-y-auto p-5 space-y-5">
        {#if !editing}
          <div class="space-y-1.5">
            <span class="label">{t('Start from a template')}</span>
            <div class="flex flex-wrap gap-1.5">
              {#each PRESETS as p (p.name)}
                <button type="button" class="chip flex items-center gap-1.5" onclick={() => usePreset(p)}><Icon name={p.icon} size={11} />{t(p.name)}</button>
              {/each}
            </div>
          </div>
        {/if}

        <div class="space-y-1.5">
          <label class="label" for="cat-name">{t('Name')}</label>
          <input id="cat-name" class="field {nameTaken ? '!border-bad' : ''}" placeholder={t('Name, for example E-books')} bind:value={name} />
          {#if nameTaken}<p class="text-[11px] text-bad">{t('A category with this name already exists.')}</p>{/if}
        </div>

        <div class="space-y-1.5">
          <span class="label">{t('Icon')}</span>
          <div class="grid grid-cols-12 gap-1.5">
            {#each ICONS as i (i)}
              <button type="button" title={i} class="aspect-square rounded-[8px] border flex items-center justify-center cursor-pointer transition-colors {icon === i ? 'border-accent bg-accent/15 text-accent' : 'border-line text-muted hover:bg-hover hover:text-white'}" onclick={() => (icon = i)}><Icon name={i} size={15} /></button>
            {/each}
          </div>
        </div>

        <div class="space-y-1.5">
          <span class="label">{t('File types')}</span>
          <div class="field !h-auto min-h-[38px] py-1.5 flex flex-wrap items-center gap-1.5 cursor-text" role="presentation" onclick={(e) => (e.currentTarget.querySelector('input') as HTMLInputElement | null)?.focus()}>
            {#each exts as e (e)}
              <span class="h-[24px] pl-2 pr-1 rounded-full text-[11.5px] mono flex items-center gap-1 {taken.includes(e) ? 'bg-warn/15 text-warn' : 'bg-accent/15 text-accent'}">.{e}<button type="button" class="w-[16px] h-[16px] rounded-full hover:bg-white/15 flex items-center justify-center" aria-label={t('Remove')} onclick={() => (exts = exts.filter((x) => x !== e))}><Icon name="x" size={9} /></button></span>
            {/each}
            <input class="flex-1 min-w-[140px] bg-transparent outline-none text-[12.5px] mono" placeholder={exts.length ? '' : 'epub mobi azw3'} bind:value={extInput} onkeydown={onExtKey} onblur={() => { addExts(extInput); extInput = ''; }} />
          </div>
          <p class="text-[11px] {taken.length ? 'text-warn' : 'text-muted'}">{taken.length ? t('Already used by another category: {list}. The first category in the list wins.', { list: taken.map((x) => '.' + x).join(' ') }) : t('Type an extension and press Enter or Space. Several at once work too.')}</p>
        </div>

        <div class="space-y-2">
          <span class="label">{t('Save to')}</span>
          <label class="flex items-start gap-2 text-[12.5px] cursor-pointer"><input type="radio" class="mt-[3px]" checked={!ownFolder} onchange={() => (ownFolder = false)} /><span>{t('A folder named after the category inside the download folder')}</span></label>
          <label class="flex items-start gap-2 text-[12.5px] cursor-pointer"><input type="radio" class="mt-[3px]" checked={ownFolder} onchange={() => (ownFolder = true)} /><span>{t('A folder of my choice')}</span></label>
          {#if ownFolder}
            <div class="flex gap-2 pl-6">
              <input class="field flex-1 min-w-0" bind:value={folder} placeholder={t('Choose a folder')} />
              <button type="button" class="btn" onclick={browse}><Icon name="folder" size={13} />{t('Browse')}</button>
            </div>
          {/if}
          {#if !app.settings?.organizeByCategory}
            <p class="text-[11px] text-warn pl-6">{t('Files are only sorted into folders while “Sort into category folders” is on in Settings → Files. The category is listed in the sidebar either way.')}</p>
          {/if}
        </div>

        <div class="rounded-[10px] border border-line bg-bg/60 p-3 space-y-1.5">
          <span class="label">{t('Preview')}</span>
          <div class="h-[30px] px-[10px] rounded-[8px] bg-line text-white flex items-center gap-[10px] text-[12.5px] max-w-[220px]"><Icon name={icon} size={15} /><span class="truncate">{name.trim() || t('Name')}</span></div>
          <div class="text-[11px] text-muted mono truncate" title={target}>{target}</div>
        </div>
      </div>

      <div class="flex items-center gap-2 px-5 py-3 border-t border-line">
        {#if editing}
          <button type="button" class="btn hover:!text-bad" onclick={remove}><Icon name="trash" size={12} />{t('Remove')}</button>
        {/if}
        <div class="flex-1"></div>
        <button type="button" class="btn" onclick={close}>{t('Cancel')}</button>
        <button type="submit" class="btn btn-primary" disabled={!canSave}>{editing ? t('Save') : t('Add category')}</button>
      </div>
    </form>
  </div>
{/if}
