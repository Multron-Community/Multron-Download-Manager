<script lang="ts">
  import { app } from '../lib/store.svelte';
  import { api, errorText } from '../lib/api';
  import { t } from '../lib/i18n.svelte';
  import { tagStyle, signatureView } from '../lib/tags';
  import type { Download } from '../types';
  import Icon from './Icon.svelte';

  let { item }: { item: Download } = $props();

  let tags = $state<string[]>([]);
  let note = $state('');
  let mirrors = $state('');
  let tagInput = $state('');
  let lastId = '';
  let dirty = $state(false);

  $effect(() => {
    if (item.id !== lastId) {
      lastId = item.id;
      tags = [...(item.tags ?? [])];
      note = item.note ?? '';
      mirrors = (item.mirrors ?? []).join('\n');
      dirty = false;
    }
  });

  let known = $derived([...new Set(app.downloads.flatMap((d) => d.tags ?? []))].filter((x) => !tags.includes(x)).slice(0, 12));
  let sig = $derived(signatureView(item.signature));

  function addTag(text: string) {
    const v = text.trim().replace(/^#/, '');
    if (v && !tags.includes(v) && tags.length < 20) {
      tags = [...tags, v];
      dirty = true;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ',') {
      e.preventDefault();
      addTag(tagInput);
      tagInput = '';
    } else if (e.key === 'Backspace' && !tagInput && tags.length) {
      tags = tags.slice(0, -1);
      dirty = true;
    }
  }

  async function save() {
    if (tagInput.trim()) {
      addTag(tagInput);
      tagInput = '';
    }
    try {
      await api.setDownloadMeta(item.id, tags, note);
      if (item.kind === 'http') await api.setDownloadMirrors(item.id, mirrors.split('\n').map((m) => m.trim()).filter(Boolean));
      dirty = false;
      app.toast('success', t('Saved'));
    } catch (e) {
      app.toast('error', errorText(e));
    }
  }
</script>

<div class="grid grid-cols-[130px_minmax(0,1fr)] gap-x-4 gap-y-3 max-w-[980px] text-[12px]">
  <span class="text-muted pt-1.5">{t('Tags')}</span>
  <div class="space-y-1.5">
    <div class="field !h-auto min-h-[34px] py-1 flex flex-wrap items-center gap-1.5">
      {#each tags as tag (tag)}
        <span class="h-[22px] pl-2 pr-1 rounded-full border text-[11.5px] flex items-center gap-1" style={tagStyle(tag)}>{tag}<button type="button" class="w-[15px] h-[15px] rounded-full hover:bg-white/15 flex items-center justify-center" aria-label={t('Remove')} onclick={() => { tags = tags.filter((x) => x !== tag); dirty = true; }}><Icon name="x" size={9} /></button></span>
      {/each}
      <input class="flex-1 min-w-[120px] bg-transparent outline-none text-[12px]" placeholder={tags.length ? '' : t('Add a tag and press Enter')} bind:value={tagInput} onkeydown={onKey} />
    </div>
    {#if known.length}
      <div class="flex flex-wrap gap-1">
        {#each known as k (k)}
          <button type="button" class="h-[20px] px-2 rounded-full border text-[11px] opacity-70 hover:opacity-100" style={tagStyle(k)} onclick={() => addTag(k)}>+ {k}</button>
        {/each}
      </div>
    {/if}
  </div>
  <span class="text-muted pt-1.5">{t('Note')}</span>
  <textarea class="field !h-[64px] py-1.5 resize-y text-[12px]" maxlength="2000" placeholder={t('Anything you want to remember about this download')} bind:value={note} oninput={() => (dirty = true)}></textarea>
  {#if item.kind === 'http'}
    <span class="text-muted pt-1.5">{t('Mirror links')}</span>
    <div class="space-y-1">
      <textarea class="field !h-[64px] py-1.5 resize-y mono text-[11.5px]" placeholder="https://mirror.example.com/file.zip" bind:value={mirrors} oninput={() => (dirty = true)}></textarea>
      <p class="text-[11px] text-muted">{t('One per line. If the download fails, it continues from the next mirror by itself.')}</p>
    </div>
  {/if}
  {#if sig}
    <span class="text-muted">{t('Digital signature')}</span>
    <span class="flex items-center gap-1.5 {sig.tone === 'ok' ? 'text-ok' : sig.tone === 'warn' ? 'text-warn' : 'text-bad'}"><Icon name={sig.valid ? 'shield' : 'alert'} size={12} />{sig.valid ? t('Signed by {name}', { name: sig.text }) : t(sig.text)}</span>
  {/if}
  <span></span>
  <div><button class="btn btn-primary !h-[28px]" disabled={!dirty && !tagInput.trim()} onclick={save}>{t('Save')}</button></div>
</div>
