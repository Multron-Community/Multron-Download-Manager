<script lang="ts">
  import { app } from '../lib/store.svelte';
  import { api, errorText } from '../lib/api';
  import { LANGUAGES, t } from '../lib/i18n.svelte';
  import Icon from './Icon.svelte';
  import { cloudView } from '../lib/cloud';
  import CloudScanWindow from './CloudScanWindow.svelte';
  import StartConfirm from './StartConfirm.svelte';
  import type { Download } from '../types';

  let deleteFiles = $state(false);
  let removeOpen = false;
  $effect(() => {
    const open = app.removeTargets.length > 0;
    if (open && !removeOpen) deleteFiles = app.settings?.deleteFilesByDefault ?? false;
    removeOpen = open;
  });

  async function answerCategoryLanguage(rename: boolean) {
    app.categoryLanguage = null;
    try {
      const moved = await api.applyCategoryLanguage(rename);
      if (rename && moved > 0) app.toast('success', t('{count} folders renamed', { count: moved }));
      app.settings = await api.getSettings();
    } catch (e) {
      app.toast('error', errorText(e));
    }
  }

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

  let loginUser = $state('');
  let loginPass = $state('');
  let loginRemember = $state(true);

  function focusOnMount(node: HTMLElement) {
    node.focus();
  }

  let pwChoice = $state<number | 'all' | 'other'>(0);
  let pwOther = $state('');
  let pwShow = $state(false);
  let savedPasswords = $derived((app.settings?.extractPasswords ?? []).map((p) => p.trim()).filter(Boolean));

  $effect(() => {
    if (app.passwordFor) {
      pwChoice = 0;
      pwOther = '';
      pwShow = false;
    }
  });

  async function submitPassword(d: Download) {
    const password = pwChoice === 'all' ? null : pwChoice === 'other' ? pwOther.trim() : savedPasswords[pwChoice];
    if (pwChoice === 'other' && !password) return;
    app.passwordFor = null;
    await api.extractWithPassword(d.id, password ?? null).catch((e) => app.toast('error', String(e)));
  }

  async function submitLogin(d: Download) {
    const ok = await app.run(() => api.setDownloadLogin(d.id, loginUser.trim(), loginPass, loginRemember));
    if (ok) {
      app.loginFor = null;
      loginUser = '';
      loginPass = '';
    }
  }


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

{#if app.passwordFor}
  {@const d = app.passwordFor}
  <div class="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4" role="presentation" onclick={(e) => e.target === e.currentTarget && (app.passwordFor = null)}>
    <form class="w-full max-w-[440px] bg-menu border border-line rounded-[14px] shadow-2xl p-5 space-y-3" onsubmit={(e) => { e.preventDefault(); submitPassword(d); }}>
      <div class="flex items-start gap-3">
        <div class="w-[38px] h-[38px] rounded-full bg-warn/15 text-warn flex items-center justify-center flex-shrink-0"><Icon name="lock" size={17} /></div>
        <div class="min-w-0">
          <div class="text-[14px] font-semibold">{t('Choose a password')}</div>
          <div class="text-[12px] text-muted truncate" title={d.name}>{d.name}</div>
        </div>
      </div>
      <div class="flex items-center justify-between">
        <span class="text-[11.5px] text-dim">{t('Saved passwords')}</span>
        <button type="button" class="text-[11.5px] text-accent hover:underline flex items-center gap-1" onclick={() => (pwShow = !pwShow)}><Icon name="eye" size={11} />{pwShow ? t('Hide') : t('Show')}</button>
      </div>
      <div class="max-h-[240px] overflow-y-auto rounded-[10px] border border-line">
        {#each savedPasswords as p, i (i)}
          <label class="flex items-center gap-2.5 px-3 h-[36px] border-b border-line/60 cursor-pointer hover:bg-hover {pwChoice === i ? 'bg-accent/10' : ''}">
            <input type="radio" name="pw" class="check" checked={pwChoice === i} onchange={() => (pwChoice = i)} />
            <span class="mono text-[12.5px] truncate">{pwShow ? p : '•'.repeat(Math.min(12, Math.max(6, p.length)))}</span>
          </label>
        {/each}
        <label class="flex items-center gap-2.5 px-3 h-[36px] border-b border-line/60 cursor-pointer hover:bg-hover {pwChoice === 'all' ? 'bg-accent/10' : ''}">
          <input type="radio" name="pw" class="check" checked={pwChoice === 'all'} onchange={() => (pwChoice = 'all')} />
          <span class="text-[12.5px]">{t('Try all saved passwords')}</span>
        </label>
        <label class="flex items-center gap-2.5 px-3 h-[40px] cursor-pointer hover:bg-hover {pwChoice === 'other' ? 'bg-accent/10' : ''}">
          <input type="radio" name="pw" class="check" checked={pwChoice === 'other'} onchange={() => (pwChoice = 'other')} />
          <input class="field !h-[28px] flex-1 min-w-0 mono text-[12px]" type={pwShow ? 'text' : 'password'} autocomplete="off" placeholder={t('Another password…')} bind:value={pwOther} onfocus={() => (pwChoice = 'other')} />
        </label>
      </div>
      <p class="text-[11px] text-dim">{t('A new password you type here is saved for next time.')}</p>
      <div class="flex justify-end gap-2 pt-1">
        <button type="button" class="btn" onclick={() => (app.passwordFor = null)}>{t('Cancel')}</button>
        <button type="submit" class="btn btn-primary" disabled={pwChoice === 'other' && !pwOther.trim()}><Icon name="archive" size={12} />{t('Extract')}</button>
      </div>
    </form>
  </div>
{/if}

{#if app.loginFor}
  {@const d = app.loginFor}
  <div class="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4" role="presentation" onclick={(e) => e.target === e.currentTarget && (app.loginFor = null)}>
    <form class="w-full max-w-[400px] bg-menu border border-line rounded-[14px] shadow-2xl p-5 space-y-3" onsubmit={(e) => { e.preventDefault(); submitLogin(d); }}>
      <div class="flex items-start gap-3">
        <div class="w-[38px] h-[38px] rounded-full bg-accent/15 text-accent flex items-center justify-center flex-shrink-0"><Icon name="lock" size={17} /></div>
        <div class="min-w-0">
          <h2 class="text-[15px] font-semibold">{t('This site needs a login')}</h2>
          <p class="text-[12px] text-muted truncate" title={d.url}>{host(d.url)}</p>
        </div>
      </div>
      <input class="field" placeholder={t('User name')} autocomplete="username" bind:value={loginUser} use:focusOnMount />
      <input class="field" type="password" placeholder={t('Password')} autocomplete="current-password" bind:value={loginPass} />
      <label class="flex items-center gap-2 text-[12px] text-muted"><input type="checkbox" class="check" bind:checked={loginRemember} />{t('Remember for this site, so its other downloads log in by themselves')}</label>
      <p class="text-[11px] text-dim">{t('The login is only sent over HTTPS. Saved passwords are encrypted with your Windows account.')}</p>
      <div class="flex justify-end gap-2">
        <button type="button" class="btn" onclick={() => (app.loginFor = null)}>{t('Cancel')}</button>
        <button type="submit" class="btn btn-primary" disabled={!loginUser.trim()}>{t('Log in and retry')}</button>
      </div>
    </form>
  </div>
{/if}

{#if app.vtResult}
  {@const r = app.vtResult}
  {@const bad = r.malicious > 0}
  {@const warn = !bad && r.suspicious > 0}
  <div class="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4" role="presentation" onclick={(e) => e.target === e.currentTarget && (app.vtResult = null)}>
    <div class="w-full max-w-[440px] bg-menu border border-line rounded-[14px] shadow-2xl overflow-hidden" role="dialog" aria-modal="true" aria-label="VirusTotal">
      <div class="p-5 space-y-4">
        <div class="flex items-start gap-3">
          <div class="w-[44px] h-[44px] rounded-full flex items-center justify-center flex-shrink-0 {r.state !== 'found' ? 'bg-accent/15 text-accent' : bad ? 'bg-bad/15 text-bad' : warn ? 'bg-warn/15 text-warn' : 'bg-ok/15 text-ok'}">
            <Icon name={bad || warn ? 'alert' : 'shield'} size={20} />
          </div>
          <div class="min-w-0">
            <div class="text-[11px] text-muted">VirusTotal</div>
            <h2 class="text-[15px] font-semibold">
              {#if r.state === 'queued'}{t('Uploaded, the analysis is still running')}{:else if bad}{t('{count} security vendors flagged this file', { count: r.malicious })}{:else if warn}{t('{count} security vendors find this file suspicious', { count: r.suspicious })}{:else}{t('No security vendor flagged this file')}{/if}
            </h2>
            <div class="text-[12px] text-muted mono truncate" title={r.name}>{r.name}</div>
          </div>
        </div>
        {#if r.state === 'found'}
          <div class="space-y-1.5">
            <div class="flex items-baseline justify-between text-[12px]"><span class="text-muted">{t('Detections')}</span><span class="mono text-[18px] font-bold {bad ? 'text-bad' : warn ? 'text-warn' : 'text-ok'}">{r.malicious + r.suspicious} / {r.engines}</span></div>
            <div class="w-full h-[8px] bg-line rounded-full overflow-hidden flex">
              <div class="h-full bg-bad" style="width: {r.engines ? (r.malicious / r.engines) * 100 : 0}%"></div>
              <div class="h-full bg-warn" style="width: {r.engines ? (r.suspicious / r.engines) * 100 : 0}%"></div>
            </div>
            {#if r.label}<div class="text-[12px]"><span class="text-muted">{t('Threat name')}:</span> <span class="mono text-bad">{r.label}</span></div>{/if}
          </div>
        {/if}
        <div class="text-[11px] text-dim mono break-all select-text">SHA-256 {r.sha256}</div>
        <div class="flex justify-end gap-2">
          <button class="btn" onclick={() => api.openUrl(r.link).catch(() => {})}><Icon name="globe" size={12} />{t('Open on VirusTotal')}</button>
          <button class="btn btn-primary" onclick={() => (app.vtResult = null)}>{t('Close')}</button>
        </div>
      </div>
    </div>
  </div>
{/if}

{#if app.captchaPanel && app.captchas.length}
  <div class="download-card fixed bottom-9 right-3 z-40 w-[min(380px,calc(100vw-24px))] rounded-[12px] border border-line bg-menu shadow-2xl overflow-hidden">
    <div class="px-4 py-2.5 flex items-center gap-2 border-b border-line">
      <Icon name="lock" size={14} class="text-warn" />
      <span class="text-[12.5px] font-semibold flex-1">{t('Sites that wait for you to solve a CAPTCHA')}</span>
      <button class="icon-btn" aria-label={t('Close')} onclick={() => (app.captchaPanel = false)}><Icon name="x" size={12} /></button>
    </div>
    <div class="max-h-[260px] overflow-y-auto">
      {#each app.captchas as c (c.id)}
        {@const left = Math.max(0, Math.round(((app.settings?.captchaTimeoutSecs ?? 300) * 1000 - (Date.now() - c.createdAt)) / 1000))}
        <div class="px-4 py-2.5 flex items-center gap-3 border-b border-line/50 last:border-0">
          <div class="min-w-0 flex-1">
            <div class="text-[12.5px] font-medium truncate" title={c.url}>{c.site}</div>
            <div class="text-[11px] text-muted">{c.kind} · {t('{seconds} s left', { seconds: left })}{app.settings?.captchaAutoRetry && c.download ? ' · ' + t('retrying by itself') : ''}</div>
          </div>
          <button class="btn btn-accent !h-[26px] !text-[11.5px]" onclick={() => api.captchaOpen(c.id).catch((e) => app.toast('error', String(e)))}>{t('Solve')}</button>
          <button class="icon-btn" title={t('Dismiss')} aria-label={t('Dismiss')} onclick={() => api.captchaDismiss(c.id)}><Icon name="x" size={12} /></button>
        </div>
      {/each}
    </div>
    <div class="px-4 py-2 text-[10.5px] text-dim border-t border-line">{t('You solve the CAPTCHA in your browser. Multron Download Manager never solves it by itself.')}</div>
  </div>
{/if}

{#if app.startTarget}
  {@const d = app.startTarget}
  <div
    class="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4"
    role="presentation"
    onclick={(e) => e.target === e.currentTarget && (app.startTarget = null)}
  >
    <StartConfirm {d} onCancel={() => (app.startTarget = null)} onConfirm={() => app.startConfirmed()} onScanFirst={scanFirst} />
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

{#if app.grabbed.length}
  <div class="download-card fixed bottom-9 left-1/2 -translate-x-1/2 z-[70] w-[min(520px,calc(100vw-32px))] bg-raised border border-line rounded-[12px] shadow-2xl pl-3 pr-2 py-2 flex gap-3 items-center">
    <div class="w-[32px] h-[32px] rounded-full flex items-center justify-center flex-shrink-0 bg-accent/20 text-accent"><Icon name="link" size={15} /></div>
    <div class="min-w-0 flex-1">
      <div class="text-[12.5px] font-semibold">{t('Link grabber')} · <span class="text-accent">{app.grabbed.length === 1 ? t('1 link copied') : t('{count} links copied', { count: app.grabbed.length })}</span></div>
      <div class="text-[11px] text-dim truncate" title={app.grabbed[app.grabbed.length - 1]}>{app.grabbed[app.grabbed.length - 1]}</div>
    </div>
    <button class="btn btn-accent !h-[28px] !text-[11.5px] flex-shrink-0" onclick={() => { app.view = 'grabber'; app.grabbed = []; }}>{t('Show')}</button>
    <button class="icon-btn flex-shrink-0" title={t('Clear')} aria-label={t('Clear')} onclick={() => (app.grabbed = [])}><Icon name="x" size={13} /></button>
  </div>
{/if}

{#if app.categoryLanguage}
  {@const c = app.categoryLanguage}
  {@const langName = LANGUAGES.find((l) => l.code === c.to)?.name ?? c.to}
  <div class="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4" role="presentation">
    <div class="w-full max-w-[440px] bg-menu border border-line rounded-[14px] shadow-2xl p-5 space-y-3" role="dialog" aria-modal="true" aria-label={t('Rename the category folders?')}>
      <h2 class="text-[15px] font-semibold">{t('Rename the category folders?')}</h2>
      <p class="text-[12.5px] text-muted leading-relaxed">{t('The app language is now {language}. Rename the category folders in your download folder to match? Downloads in them move along.', { language: langName })}</p>
      <ul class="text-[12px] rounded-[10px] border border-line bg-raised px-3 py-2 space-y-1 max-h-[180px] overflow-auto">
        {#each c.folders as [from, to]}
          <li class="flex items-center gap-2 mono truncate"><span class="text-muted truncate">{from}</span><span class="text-dim">→</span><span class="truncate">{to}</span></li>
        {/each}
      </ul>
      <div class="flex justify-end gap-2">
        <button class="btn" onclick={() => answerCategoryLanguage(false)}>{t('Keep current names')}</button>
        <button class="btn btn-accent" onclick={() => answerCategoryLanguage(true)}>{t('Rename folders')}</button>
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
    <div class="toast-in pointer-events-auto border {toastStyle[toast.kind]} rounded-[12px] px-3 py-2.5 flex gap-2 items-start text-[12px]">
      <Icon name={toastIcon[toast.kind]} size={14} class="mt-[1px] flex-shrink-0 {toast.kind === 'error' ? 'text-bad' : toast.kind === 'success' ? 'text-ok' : toast.kind === 'warning' ? 'text-warn' : 'text-muted'}" />
      <span class="break-words min-w-0 select-text">{toast.message}</span>
    </div>
  {/each}
</div>
