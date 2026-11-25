<script lang="ts">
  import { app } from '../lib/store.svelte';
  import { PROGRAM_FILE } from '../lib/files';
  import { bytes } from '../lib/format';
  import { t } from '../lib/i18n.svelte';
  import { cloudView } from '../lib/cloud';
  import { signatureView } from '../lib/tags';
  import type { Download } from '../types';
  import Icon from './Icon.svelte';

  let {
    d,
    windowed = false,
    onCancel,
    onConfirm,
    onScanFirst
  }: {
    d: Download;
    windowed?: boolean;
    onCancel: () => void;
    onConfirm: () => void;
    onScanFirst: () => void;
  } = $props();

  const toneText: Record<string, string> = { ok: 'text-ok', warn: 'text-warn', bad: 'text-bad', muted: 'text-muted' };
  const toneBox: Record<string, string> = { ok: 'bg-ok/10 border-ok/30', warn: 'bg-warn/10 border-warn/40', bad: 'bg-bad/10 border-bad/40', muted: 'bg-line/40 border-line' };

  function host(url: string) {
    try {
      return new URL(url).host;
    } catch {
      return '';
    }
  }

  let result = $derived.by(() => {
    const v = cloudView(d.cloudState, d.cloudDetail);
    return { tone: v.tone, icon: v.icon === 'lock' ? 'lock' : v.icon, text: v.label, reason: v.reason };
  });
  let danger = $derived(result.tone === 'bad' || d.scanState === 'threat');
  let shield = $derived(app.platform === 'macos' ? 'Gatekeeper' : 'SmartScreen');
  let program = $derived(PROGRAM_FILE.test(d.name) || app.disguised(d));
  let sv = $derived(signatureView(d.signature));
</script>

<div
  class={windowed ? 'h-full flex flex-col' : 'w-full max-w-[440px] bg-menu border border-line rounded-[14px] shadow-2xl overflow-hidden'}
  role="dialog"
  aria-modal="true"
  aria-label={program ? t('Start this program?') : t('Open this file?')}
>
  <div class="p-5 space-y-4 {windowed ? 'flex-1 overflow-y-auto' : ''}">
    <div class="flex items-start gap-3">
      <div class="w-[40px] h-[40px] rounded-full flex items-center justify-center flex-shrink-0 {danger ? 'bg-bad/15 text-bad' : 'bg-warn/15 text-warn'}">
        <Icon name="alert" size={19} />
      </div>
      <div class="min-w-0">
        <h2 class="text-[15px] font-semibold">{program ? t('Start this program?') : t('Open this file?')}</h2>
        <p class="text-[12.5px] text-muted leading-relaxed mt-0.5">{program ? t('{name} was downloaded from the internet. Start it only if you trust where it came from.', { name: d.name }) : t('{name} was downloaded from the internet. Open it only if you trust where it came from.', { name: d.name })}</p>
      </div>
    </div>
    <div class="rounded-[10px] border border-line bg-raised px-3 py-2.5 flex items-center gap-3">
      <div class="w-[34px] h-[34px] rounded-[8px] bg-line/60 flex items-center justify-center text-[10px] font-bold flex-shrink-0">{(d.name.split('.').pop() ?? '').slice(0, 4).toUpperCase()}</div>
      <div class="min-w-0">
        <div class="text-[12.5px] font-medium mono truncate" title={d.name}>{d.name}</div>
        <div class="text-[11.5px] text-muted truncate">{bytes(d.total)}{host(d.url) ? ' · ' + host(d.url) : ''}</div>
      </div>
    </div>
    {#if sv}
      <div class="rounded-[10px] border px-3 py-2 flex items-center gap-2 text-[12.5px] {sv.valid ? toneBox.ok : sv.tone === 'bad' ? toneBox.bad : toneBox.warn}">
        <Icon name={sv.valid ? 'shield' : 'alert'} size={14} class={sv.valid ? 'text-ok' : sv.tone === 'bad' ? 'text-bad' : 'text-warn'} />
        <span class="font-medium">{t('Digital signature')}</span>
        <span class="ml-auto text-right truncate {sv.valid ? 'text-ok' : sv.tone === 'bad' ? 'text-bad' : 'text-warn'}">{sv.valid ? sv.text : t(sv.text)}</span>
      </div>
    {/if}
    {#if app.disguised(d)}
      <div class="rounded-[10px] border px-3 py-2 flex items-start gap-2 text-[12px] {toneBox.bad}">
        <Icon name="alert" size={14} class="text-bad mt-[1px] flex-shrink-0" />
        <span>{t('The name of this file hides that it is a program, for example “invoice.pdf.exe”. This is a common trick of malware.')}</span>
      </div>
    {/if}
    <div class="space-y-1.5">
      <div class="rounded-[10px] border px-3 py-2 flex items-center gap-2 text-[12.5px] {toneBox[result.tone]}">
        <Icon name={result.icon} size={14} class={toneText[result.tone]} />
        <span class="font-medium">Multron Cloud</span>
        <span class="ml-auto {toneText[result.tone]}">{result.text}</span>
      </div>
      {#if result.reason}
        <div class="text-[11.5px] text-muted px-1 select-text leading-relaxed">{result.reason}</div>
      {/if}
      {#if d.scanState === 'threat'}
        <div class="rounded-[10px] border px-3 py-2 flex items-center gap-2 text-[12.5px] {toneBox.bad}">
          <Icon name="alert" size={14} class="text-bad" />
          <span class="font-medium">{t('Antivirus')}</span>
          <span class="ml-auto text-bad">{t('Blocked by your antivirus')}</span>
        </div>
      {:else if d.scanState === 'marked'}
        <div class="rounded-[10px] border px-3 py-2 flex items-center gap-2 text-[12.5px] {toneBox.ok}">
          <Icon name="shield" size={14} class="text-ok" />
          <span class="font-medium">{shield}</span>
          <span class="ml-auto text-ok text-right">{t('Checks the file when it starts')}</span>
        </div>
      {/if}
    </div>
  </div>
  <div class="flex items-center gap-2 px-5 py-3 border-t border-line">
    {#if ['', 'skipped', 'error', 'partial'].includes(d.cloudState) && app.settings?.cloudScanButton !== false}
      <button class="btn" onclick={onScanFirst}><Icon name="shield" size={13} />{t('Scan first')}</button>
    {/if}
    <div class="flex gap-2 ml-auto">
      <button class="btn" onclick={onCancel}>{t('Cancel')}</button>
      <button class="btn {danger ? 'btn-danger' : 'btn-accent'}" onclick={onConfirm}>{danger ? (program ? t('Start anyway') : t('Open anyway')) : program ? t('Start') : t('Open')}</button>
    </div>
  </div>
</div>
