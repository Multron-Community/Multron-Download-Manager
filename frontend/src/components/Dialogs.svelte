<script lang="ts">
  import { app } from '../lib/store.svelte';
  import { api, errorText } from '../lib/api';
  import { t } from '../lib/i18n.svelte';
  import Icon from './Icon.svelte';

  let deleteFiles = $state(false);

  async function confirmRemove() {
    const targets = app.removeTargets;
    if (!targets.length) return;
    app.removeTargets = [];
    const ids = targets.map((d) => d.id);
    if (app.selectedId && ids.includes(app.selectedId)) app.selectedId = null;
    app.checked = app.checked.filter((id) => !ids.includes(id));
    const files = deleteFiles;
    deleteFiles = false;
    if (ids.length === 1) await app.run(() => api.remove(ids[0], files));
    else await app.run(() => api.removeMany(ids, files));
  }

  async function answerPair(origin: string, allow: boolean) {
    try {
      app.settings = await api.approveExtension(origin, allow);
      if (allow) app.toast('success', 'Browser extension connected');
    } catch (e) {
      app.toast('error', errorText(e));
    }
    app.pairOrigins = app.pairOrigins.filter((o) => o !== origin);
  }

  function browserName(origin: string) {
    if (origin.startsWith('moz-extension://')) return 'Firefox';
    if (origin.startsWith('safari-web-extension://')) return 'Safari';
    return 'Chrome / Edge / Brave / Opera';
  }

  const toastStyle: Record<string, string> = {
    info: 'border-line',
    success: 'border-ok/50',
    warning: 'border-warn/50',
    error: 'border-bad/60'
  };
  const toastIcon: Record<string, string> = { info: 'info', success: 'check', warning: 'alert', error: 'alert' };
</script>

{#if app.removeTargets.length}
  {@const targets = app.removeTargets}
  <div class="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4" role="presentation">
    <div class="w-full max-w-[420px] bg-menu border border-line rounded-[14px] shadow-2xl p-5 space-y-4">
      <h2 class="text-[15px] font-semibold">{targets.length === 1 ? t('Remove download') : t('Remove {count} downloads', { count: targets.length })}</h2>
      <ul class="text-[12.5px] text-muted space-y-0.5 max-h-[140px] overflow-y-auto">
        {#each targets.slice(0, 6) as d (d.id)}
          <li class="truncate" title={d.name}>{d.name}</li>
        {/each}
        {#if targets.length > 6}
          <li>{t('and {count} more', { count: targets.length - 6 })}</li>
        {/if}
      </ul>
      <label class="flex items-center gap-2 text-[12.5px]">
        <input type="checkbox" class="check" bind:checked={deleteFiles} />{t('Also delete the downloaded files')}
      </label>
      <div class="flex justify-end gap-2">
        <button class="btn" onclick={() => (app.removeTargets = [])}>{t('Cancel')}</button>
        <button class="btn btn-danger" onclick={confirmRemove}>{t('Remove')}</button>
      </div>
    </div>
  </div>
{/if}

{#if app.pairOrigins.length}
  {@const origin = app.pairOrigins[0]}
  <div class="fixed inset-0 bg-black/60 backdrop-blur-sm z-[60] flex items-center justify-center p-4" role="presentation">
    <div class="w-full max-w-[460px] bg-menu border border-line rounded-[14px] shadow-2xl p-5 space-y-4">
      <div class="flex items-center gap-3">
        <div class="w-10 h-10 rounded-full bg-accent-strong/25 text-accent flex items-center justify-center"><Icon name="puzzle" size={18} /></div>
        <div>
          <h2 class="text-[15px] font-semibold">{t('Connect browser extension?')}</h2>
          <p class="text-[12px] text-muted">{browserName(origin)}</p>
        </div>
      </div>
      <p class="text-[12.5px] text-muted leading-relaxed">
        {t('A Multron Download Manager browser extension asks to send downloads to this app. Only allow it if you just installed the extension or pressed “Connect”.')}
      </p>
      <p class="mono text-[11px] text-dim break-all">{origin}</p>
      <div class="flex justify-end gap-2">
        <button class="btn" onclick={() => answerPair(origin, false)}>{t('Deny')}</button>
        <button class="btn btn-accent" onclick={() => answerPair(origin, true)}>{t('Allow')}</button>
      </div>
    </div>
  </div>
{/if}

<div class="fixed bottom-9 right-4 z-[70] flex flex-col gap-2 w-[340px] pointer-events-none">
  {#each app.toasts as toast (toast.id)}
    <div class="pointer-events-auto bg-raised border {toastStyle[toast.kind]} rounded-[10px] shadow-xl px-3 py-2.5 flex gap-2 items-start text-[12px]">
      <Icon name={toastIcon[toast.kind]} size={14} class="mt-[1px] flex-shrink-0 {toast.kind === 'error' ? 'text-bad' : toast.kind === 'success' ? 'text-ok' : toast.kind === 'warning' ? 'text-warn' : 'text-muted'}" />
      <span class="break-words min-w-0 select-text">{toast.message}</span>
    </div>
  {/each}
</div>
