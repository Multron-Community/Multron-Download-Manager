<script lang="ts">
  import { app } from '../lib/store.svelte';
  import { api, errorText } from '../lib/api';
  import { bytes } from '../lib/format';
  import { t } from '../lib/i18n.svelte';
  import Icon from './Icon.svelte';
  import { cloudView } from '../lib/cloud';
  import CloudScanWindow from './CloudScanWindow.svelte';

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

  async function acceptCloudEula() {
    const id = app.cloudEulaFor;
    app.cloudEulaFor = null;
    if (!id || !app.settings) return;
    try {
      app.settings = await api.saveSettings({ ...app.settings, cloudScanAccepted: true });
      if (id === 'launch') return;
      const d = app.downloads.find((x) => x.id === id);
      if (id === 'all') await app.scanAllInCloud();
      else if (d) await app.scanInCloud(d);
      else await api.cloudScan(id);
    } catch (e) {
      app.toast('error', errorText(e));
    }
  }

  async function declineCloudEula() {
    const id = app.cloudEulaFor;
    app.cloudEulaFor = null;
    if (id !== 'launch' || !app.settings) return;
    try {
      app.settings = await api.saveSettings({ ...app.settings, cloudScan: false });
    } catch (e) {
      app.toast('error', errorText(e));
    }
  }

  let archivePassword = $state('');
  let passwordError = $state('');
  let passwordBusy = $state(false);

  function closePassword() {
    app.cloudPasswordFor = null;
    archivePassword = '';
    passwordError = '';
  }

  async function scanWithPassword() {
    const id = app.cloudPasswordFor;
    if (!id || !archivePassword || passwordBusy) return;
    passwordBusy = true;
    passwordError = '';
    try {
      await api.cloudScanWithPassword(id, archivePassword);
      closePassword();
      app.cloudReportFor = id;
      app.toast('info', t('Scanning the files inside the archive'));
    } catch (e) {
      passwordError = errorText(e);
    } finally {
      passwordBusy = false;
    }
  }

  function startResult(d: { cloudState: string; cloudDetail?: string | null }) {
    const v = cloudView(d.cloudState, d.cloudDetail);
    return { tone: v.tone, icon: v.icon === 'lock' ? 'lock' : v.icon, text: v.label, reason: v.reason };
  }

  function host(url: string) {
    try {
      return new URL(url).host;
    } catch {
      return '';
    }
  }

  function scanFirst() {
    const d = app.startTarget;
    app.startTarget = null;
    if (d) app.scanInCloud(d);
  }

  const toneText: Record<string, string> = { ok: 'text-ok', warn: 'text-warn', bad: 'text-bad', muted: 'text-muted' };
  const toneBox: Record<string, string> = { ok: 'bg-ok/10 border-ok/30', warn: 'bg-warn/10 border-warn/40', bad: 'bg-bad/10 border-bad/40', muted: 'bg-line/40 border-line' };

  const toastStyle: Record<string, string> = {
    info: 'border-line',
    success: 'border-ok/50',
    warning: 'border-warn/50',
    error: 'border-bad/60'
  };
  const toastIcon: Record<string, string> = { info: 'info', success: 'check', warning: 'alert', error: 'alert' };
</script>

{#if app.cloudEulaFor}
  <div class="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4" role="presentation">
    <div class="w-full max-w-[480px] bg-menu border border-line rounded-[14px] shadow-2xl p-5 space-y-4">
      <h2 class="text-[15px] font-semibold flex items-center gap-2"><Icon name="shield" size={16} />{t('Cloud Malware Scan License Agreement')}<span class="px-1.5 rounded bg-warn/15 text-warn text-[10px] font-semibold uppercase tracking-wide">{t('Testing')}</span></h2>
      <p class="text-[12.5px] text-muted leading-relaxed">{t('Multron Cloud scans each finished download separately from your antivirus, with the viruskov.com OPEN-EDR engine. The file hash is checked first; a file the cloud has not seen yet is uploaded for analysis and may be reviewed by viruskov.com analysts. The cloud scan is a test version and may report false positives, so check a detection before you delete a file.')}</p>
      <p class="text-[12px] text-muted">{t('You are asked only once. Finished programs and scripts are then scanned automatically; you can change the file types or turn it off in Options › Security.')}</p>
      {#if app.cloudEulaFor === 'launch'}
        <p class="text-[12px] text-muted">{t('If you decline, automatic scanning is turned off. You can turn it on later in Options › Security.')}</p>
      {/if}
      <button class="btn" onclick={() => api.openUrl('https://github.com/Multron-Community/MultronWcleaner/blob/beta/MALWARE_SCAN_EULA.md')}><Icon name="globe" size={13} />{t('Read the agreement')}</button>
      <div class="flex justify-end gap-2 pt-1">
        <button class="btn" onclick={declineCloudEula}>{app.cloudEulaFor === 'launch' ? t('Not now') : t('Cancel')}</button>
        <button class="btn btn-accent" onclick={acceptCloudEula}>{app.cloudEulaFor === 'launch' ? t('Accept') : t('Accept and scan')}</button>
      </div>
    </div>
  </div>
{/if}

<CloudScanWindow />

{#if app.cloudPasswordFor}
  {@const target = app.downloads.find((d) => d.id === app.cloudPasswordFor)}
  <div class="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4" role="presentation">
    <div class="w-full max-w-[420px] bg-menu border border-line rounded-[14px] shadow-2xl p-5 space-y-3">
      <h2 class="text-[15px] font-semibold flex items-center gap-2"><Icon name="lock" size={16} />{t('Password-protected archive')}</h2>
      <p class="text-[12.5px] text-muted leading-relaxed">{t('Multron Cloud cannot look inside {name} without its password. Enter the password to unpack it in a private folder and scan the files inside. The password is not saved or sent anywhere.', { name: target?.name ?? '' })}</p>
      <input
        type="password"
        class="field w-full"
        placeholder={t('Archive password')}
        autocomplete="off"
        bind:value={archivePassword}
        onkeydown={(e) => e.key === 'Enter' && scanWithPassword()}
      />
      {#if passwordError}
        <p class="text-[12px] text-bad flex items-center gap-1.5"><Icon name="alert" size={12} />{passwordError}</p>
      {/if}
      <div class="flex justify-end gap-2 pt-1">
        <button class="btn" onclick={closePassword}>{t('Cancel')}</button>
        <button class="btn btn-accent" disabled={!archivePassword || passwordBusy} onclick={scanWithPassword}>{passwordBusy ? t('Unpacking…') : t('Scan')}</button>
      </div>
    </div>
  </div>
{/if}

<svelte:window onkeydown={(e) => e.key === 'Escape' && app.startTarget && (app.startTarget = null)} />

{#if app.startTarget}
  {@const d = app.startTarget}
  {@const result = startResult(d)}
  {@const danger = result.tone === 'bad' || d.scanState === 'threat'}
  {@const shield = app.platform === 'macos' ? 'Gatekeeper' : 'SmartScreen'}
  <div
    class="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4"
    role="presentation"
    onclick={(e) => e.target === e.currentTarget && (app.startTarget = null)}
  >
    <div class="w-full max-w-[440px] bg-menu border border-line rounded-[14px] shadow-2xl overflow-hidden" role="dialog" aria-modal="true" aria-label={t('Start this program?')}>
      <div class="p-5 space-y-4">
        <div class="flex items-start gap-3">
          <div class="w-[40px] h-[40px] rounded-full flex items-center justify-center flex-shrink-0 {danger ? 'bg-bad/15 text-bad' : 'bg-warn/15 text-warn'}">
            <Icon name="alert" size={19} />
          </div>
          <div class="min-w-0">
            <h2 class="text-[15px] font-semibold">{t('Start this program?')}</h2>
            <p class="text-[12.5px] text-muted leading-relaxed mt-0.5">{t('{name} was downloaded from the internet. Start it only if you trust where it came from.', { name: d.name })}</p>
          </div>
        </div>
        <div class="rounded-[10px] border border-line bg-raised px-3 py-2.5 flex items-center gap-3">
          <div class="w-[34px] h-[34px] rounded-[8px] bg-line/60 flex items-center justify-center text-[10px] font-bold flex-shrink-0">{(d.name.split('.').pop() ?? '').slice(0, 4).toUpperCase()}</div>
          <div class="min-w-0">
            <div class="text-[12.5px] font-medium mono truncate" title={d.name}>{d.name}</div>
            <div class="text-[11.5px] text-muted truncate">{bytes(d.total)}{host(d.url) ? ' · ' + host(d.url) : ''}</div>
          </div>
        </div>
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
          <button class="btn" onclick={scanFirst}><Icon name="shield" size={13} />{t('Scan first')}</button>
        {/if}
        <div class="flex gap-2 ml-auto">
          <button class="btn" onclick={() => (app.startTarget = null)}>{t('Cancel')}</button>
          <button class="btn {danger ? 'btn-danger' : 'btn-accent'}" onclick={() => app.startConfirmed()}>{danger ? t('Start anyway') : t('Start')}</button>
        </div>
      </div>
    </div>
  </div>
{/if}

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
  {#each app.notices as n (n.id)}
    {@const d = app.downloads.find((x) => x.id === n.downloadId)}
    {@const view = n.kind === 'scan' ? cloudView(n.result ?? '', n.detail) : null}
    {@const tone = view ? (view.tone === 'muted' ? 'warn' : view.tone) : n.kind === 'started' ? 'accent' : n.kind === 'finished' ? 'ok' : 'bad'}
    <div class="pointer-events-auto bg-raised border-l-4 border border-line {tone === 'accent' ? '!border-l-accent' : tone === 'ok' ? '!border-l-ok' : tone === 'warn' ? '!border-l-warn' : '!border-l-bad'} rounded-[10px] shadow-2xl p-3 flex gap-3 items-start">
      <div class="w-[34px] h-[34px] rounded-full flex items-center justify-center flex-shrink-0 {tone === 'accent' ? 'bg-accent/20 text-accent' : tone === 'ok' ? 'bg-ok/20 text-ok' : tone === 'warn' ? 'bg-warn/20 text-warn' : 'bg-bad/20 text-bad'}">
        <Icon name={view ? (view.icon === 'shield' ? 'shield' : 'alert') : n.kind === 'started' ? 'down' : n.kind === 'finished' ? 'check' : 'alert'} size={17} />
      </div>
      <div class="min-w-0 flex-1">
        <div class="text-[13px] font-semibold">{n.kind === 'scan' ? t('Multron Cloud scan finished') : n.kind === 'started' ? t('Download started') : n.kind === 'finished' ? t('Download finished') : t('Download failed')}</div>
        <div class="text-[12px] text-muted truncate" title={n.name}>{n.name}</div>
        {#if view}
          <div class="text-[12px] font-medium mt-0.5 {tone === 'ok' ? 'text-ok' : tone === 'warn' ? 'text-warn' : 'text-bad'}" title={view.reason}>{view.label}</div>
          {#if view.reason}<div class="text-[11px] text-muted line-clamp-3 select-text" title={view.reason}>{view.reason}</div>{/if}
        {/if}
        <div class="flex gap-1.5 mt-2">
          {#if n.kind === 'finished' && d}
            <button class="btn !h-[26px] !text-[11.5px]" onclick={() => { app.dismissNotice(n.id); app.openDownload(d); }}>{t('Open')}</button>
            <button class="btn !h-[26px] !text-[11.5px]" onclick={() => { app.dismissNotice(n.id); app.run(() => api.showInFolder(n.downloadId)); }}><Icon name="folderOpen" size={12} />{t('Show in folder')}</button>
          {:else if d && n.kind !== 'scan'}
            <button class="btn !h-[26px] !text-[11.5px]" onclick={() => { app.dismissNotice(n.id); app.selectedId = n.downloadId; }}>{t('Show')}</button>
          {/if}
        </div>
      </div>
      <button class="icon-btn !w-[22px] !h-[22px]" title={t('Close')} onclick={() => app.dismissNotice(n.id)}><Icon name="x" size={12} /></button>
    </div>
  {/each}
  {#each app.toasts as toast (toast.id)}
    <div class="pointer-events-auto bg-raised border {toastStyle[toast.kind]} rounded-[10px] shadow-xl px-3 py-2.5 flex gap-2 items-start text-[12px]">
      <Icon name={toastIcon[toast.kind]} size={14} class="mt-[1px] flex-shrink-0 {toast.kind === 'error' ? 'text-bad' : toast.kind === 'success' ? 'text-ok' : toast.kind === 'warning' ? 'text-warn' : 'text-muted'}" />
      <span class="break-words min-w-0 select-text">{toast.message}</span>
    </div>
  {/each}
</div>
