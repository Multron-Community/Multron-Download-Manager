<script lang="ts">
  import { onDestroy } from 'svelte';
  import { app } from '../lib/store.svelte';
  import { api } from '../lib/api';
  import { t } from '../lib/i18n.svelte';
  import { bytes, duration } from '../lib/format';
  import type { CloudFinding } from '../types';
  import Icon from './Icon.svelte';
  import { cloudReason } from '../lib/cloud';

  let showFiles = $state(false);
  const RING = 36;
  const CIRCLE = 2 * Math.PI * RING;
  let now = $state(Date.now());
  const timer = setInterval(() => (now = Date.now()), 1000);
  onDestroy(() => clearInterval(timer));

  const report = $derived(app.cloudReportFor ? app.cloudReports[app.cloudReportFor] : undefined);
  const threats = $derived(report?.findings.filter((f) => f.verdict === 'malicious' || f.verdict === 'suspicious') ?? []);
  const files = $derived([...(report?.recent ?? [])].reverse());
  const percent = $derived(report && report.total > 0 ? Math.round((report.done / report.total) * 100) : 0);
  const elapsed = $derived(
    report?.startedMs ? Math.max(0, Math.round(((report.running || !report.finishedMs ? now : report.finishedMs) - report.startedMs) / 1000)) : 0
  );
  const remaining = $derived(
    report?.running && report.done > 0 ? Math.round((elapsed / report.done) * (report.total - report.done)) : null
  );
  const uploadPercent = $derived(report && report.stageTotal > 0 ? Math.round((report.stageSent / report.stageTotal) * 100) : 0);

  function close() {
    app.cloudReportFor = null;
  }

  let stopping = $state<string | null>(null);
  $effect(() => {
    if (!report?.running) stopping = null;
  });

  function stop() {
    if (!report) return;
    stopping = report.id;
    api.cloudScanStop(report.id).catch(() => (stopping = null));
  }

  function reveal(f: CloudFinding) {
    if (report) app.run(() => api.revealDownloadedFile(f.download || report.id, f.path));
  }

  const notScanned = $derived(!!report && !report.running && !report.stopped && report.state === 'skipped');
  const partly = $derived(!!report && !report.running && !report.stopped && report.state === 'partial');
  const reason = $derived(cloudReason(report?.summary));
  const ring = $derived(report?.running ? percent : 100);
  const tone = $derived(
    report?.threats
      ? { text: 'text-bad' }
      : report?.suspicious || notScanned || partly || report?.stopped || report?.state === 'error'
        ? { text: 'text-warn' }
        : report?.running
          ? { text: 'text-accent' }
          : { text: 'text-ok' }
  );

  function result(f: CloudFinding) {
    switch (f.verdict) {
      case 'clean':
      case 'possible_clean':
        return { text: t('Clean'), cls: 'bg-ok/15 text-ok' };
      case 'unknown':
        return { text: t('No detection'), cls: 'bg-ok/10 text-ok' };
      case 'malicious':
        return { text: t('Threat'), cls: 'bg-bad/15 text-bad' };
      case 'suspicious':
        return { text: t('Suspicious'), cls: 'bg-warn/15 text-warn' };
      case 'locked':
        return { text: t('Password needed'), cls: 'bg-warn/15 text-warn' };
      case 'skipped':
        return { text: t('Not scanned'), cls: 'bg-warn/15 text-warn' };
      default:
        return { text: t('Error'), cls: 'bg-line text-muted' };
    }
  }

  function how(f: CloudFinding) {
    if (f.route === 'hash') return t('Known fingerprint');
    if (f.route === 'upload') return t('Uploaded and analyzed');
    if (f.verdict === 'skipped') return t('Unknown fingerprint, larger than the upload limit');
    if (f.route === 'local') return t('Checked on this PC');
    return '—';
  }
</script>

{#if app.cloudReportFor}
  <div class="fixed inset-0 bg-black/55 backdrop-blur-sm z-50 flex items-center justify-center p-4" role="presentation">
    <div class="w-full max-w-[600px] max-h-[86vh] bg-menu border border-line rounded-[16px] shadow-2xl flex flex-col overflow-hidden">
      <div class="flex items-center gap-2 px-5 pt-4">
        <span class="text-[12px] font-semibold text-muted flex items-center gap-1.5"><Icon name="shield" size={13} />{t('Multron Cloud scan')}</span>
        <span class="px-1.5 rounded bg-warn/15 text-warn text-[9.5px] font-semibold uppercase tracking-wide">{t('Testing')}</span>
        <div class="flex-1"></div>
        <button class="icon-btn" title={t('Close')} onclick={close}><Icon name="x" size={14} /></button>
      </div>

      {#if !report}
        <div class="px-6 py-10 text-center text-[12.5px] text-muted">{t('Starting the scan…')}</div>
      {:else}
        <div class="px-6 pt-3 pb-5 flex items-center gap-5">
          <div class="relative w-[84px] h-[84px] flex-shrink-0">
            <svg viewBox="0 0 84 84" class="w-full h-full -rotate-90">
              <circle cx="42" cy="42" r={RING} fill="none" stroke="currentColor" stroke-width="6" class="text-line" />
              <circle cx="42" cy="42" r={RING} fill="none" stroke="currentColor" stroke-width="6" stroke-linecap="round" class="{tone.text} transition-all duration-500" stroke-dasharray={CIRCLE} stroke-dashoffset={CIRCLE * (1 - ring / 100)} />
            </svg>
            <div class="absolute inset-0 flex items-center justify-center {tone.text}">
              {#if report.running}
                <span class="text-[16px] font-semibold text-white">{percent}%</span>
              {:else}
                <Icon name={report.threats || report.suspicious ? 'alert' : notScanned || partly || report.state === 'error' || report.stopped ? 'shield' : 'check'} size={28} />
              {/if}
            </div>
          </div>
          <div class="min-w-0 flex-1">
            <h2 class="text-[18px] font-semibold leading-tight">
              {#if report.running}
                {t('Scanning {done} of {total} files', { done: Math.min(report.done + 1, report.total), total: report.total })}
              {:else if report.stopped}
                {t('Stopped after {done} of {total} files', { done: report.done, total: report.total })}
              {:else if report.threats}
                {t('Threats found')}
              {:else if report.suspicious}
                {t('Suspicious files found')}
              {:else if report.state === 'error'}
                {t('The scan could not be finished')}
              {:else if notScanned}
                {t('Not scanned')}
              {:else if partly}
                {t('Partly scanned')}
              {:else}
                {t('No threat found')}
              {/if}
            </h2>
            <div class="text-[12px] text-muted truncate mt-0.5" title={report.name ?? ''}>
              {report.id === 'all' ? t('All downloads') + (report.ids.length ? ' · ' + t('{count} downloads', { count: report.ids.length }) : '') : (report.name ?? '')}
            </div>
            <div class="text-[11px] text-dim mt-1.5 flex flex-wrap gap-x-3 gap-y-0.5">
              <span class="whitespace-nowrap">{t('Elapsed')} <b class="text-muted font-semibold">{duration(elapsed)}</b></span>
              {#if remaining != null}<span class="whitespace-nowrap">{t('Remaining')} <b class="text-muted font-semibold">{duration(remaining)}</b></span>{/if}
              {#if report.uploaded}<span class="whitespace-nowrap">{t('Uploaded to the cloud')} <b class="text-muted font-semibold">{report.uploaded} · {bytes(report.uploadedBytes)}</b></span>{/if}
              {#if report.known}<span class="whitespace-nowrap">{t('Known by fingerprint')} <b class="text-muted font-semibold">{report.known}</b></span>{/if}
            </div>
            {#if !report.running && reason && (report.state === 'error' || report.stopped || report.notChecked > 0 || report.state === 'unknown')}
              <div class="text-[11.5px] mt-1.5 select-text {report.state === 'error' ? 'text-bad' : 'text-warn'}">{reason}</div>
            {/if}
          </div>
        </div>

        {#if report.running}
          <div class="mx-5 mb-4 rounded-[10px] bg-accent/8 border border-accent/25 px-3 py-2">
            <div class="text-[11.5px] text-accent flex items-center gap-1.5">
              <Icon name={report.stage === 'uploading' ? 'up' : report.stage === 'hashing' ? 'hash' : 'shield'} size={12} class="flex-shrink-0" />
              <span class="truncate">
                {#if report.stage === 'hashing'}
                  {t('Computing fingerprints (SHA-256)… {done} of {total}', { done: report.stageSent + 1, total: report.stageTotal })}
                {:else if report.stage === 'checking'}
                  {t('Sending {count} fingerprints to Multron Cloud; only files it does not know are uploaded afterwards…', { count: report.stageTotal })}
                {:else if report.stage === 'waiting'}
                  {t('Waiting for Multron Cloud to answer about {count} files that are still being analyzed…', { count: report.stageTotal })}
                {:else if report.stage === 'uploading'}
                  {t('Uploading to Multron Cloud… {percent}% · {sent} of {total}', { percent: uploadPercent, sent: bytes(report.stageSent), total: bytes(report.stageTotal) })}
                {:else if report.stage === 'analyzing'}
                  {t('Being analyzed by the viruskov.com OPEN-EDR engine…')}
                {:else if report.stage === 'reconnecting'}
                  {t('Connection lost; reconnecting to Multron Cloud…')}
                {:else}
                  {t('Connecting to Multron Cloud…')}
                {/if}
              </span>
            </div>
            {#if report.current && report.stage !== 'checking'}
              <div class="text-[11px] mono text-muted truncate mt-0.5" title={report.current}>{report.current}</div>
            {/if}
            {#if report.stage === 'uploading'}
              <div class="h-[3px] rounded-full bg-line overflow-hidden mt-1.5"><div class="h-full bg-accent transition-all" style="width: {uploadPercent}%"></div></div>
            {/if}
          </div>
        {/if}

        <div class="px-5 pb-4 flex flex-wrap gap-1.5">
          <span class="pill bg-ok/12 text-ok"><Icon name="check" size={11} />{t('Clean')} <b>{report.clean}</b></span>
          {#if report.unknown}
            <span class="pill bg-ok/8 text-ok/85" title={t('No threat was found, but the file is not verified as clean yet')}>{t('No detection')} <b>{report.unknown}</b></span>
          {/if}
          <span class="pill {report.threats ? 'bg-bad/15 text-bad' : 'bg-line/60 text-muted'}">{t('Threats')} <b>{report.threats}</b></span>
          {#if report.suspicious}
            <span class="pill bg-warn/15 text-warn">{t('Suspicious')} <b>{report.suspicious}</b></span>
          {/if}
          {#if report.notChecked}
            <span class="pill bg-line/60 text-muted">{t('Not checked')} <b>{report.notChecked}</b></span>
          {/if}
        </div>

        <div class="flex-1 min-h-0 overflow-y-auto px-5 pb-3 space-y-1.5">
          {#each threats as f (f.path)}
            <div class="rounded-[10px] border p-2.5 flex items-center gap-2.5 {f.verdict === 'malicious' ? 'border-bad/45 bg-bad/10' : 'border-warn/45 bg-warn/10'}">
              <Icon name="alert" size={15} class="flex-shrink-0 {f.verdict === 'malicious' ? 'text-bad' : 'text-warn'}" />
              <div class="min-w-0 flex-1">
                <div class="text-[12px] mono truncate" title={f.name}>{f.name}</div>
                <div class="text-[11px] {f.verdict === 'malicious' ? 'text-bad' : 'text-warn'} truncate select-text" title={f.detail ?? ''}>{f.verdict === 'malicious' ? t('Threat found') : t('Suspicious file')}{f.detail ? ' · ' + f.detail : ''}</div>
              </div>
              <button class="btn !h-[26px] !text-[11.5px] flex-shrink-0" onclick={() => reveal(f)}><Icon name="folderOpen" size={12} />{t('Show in folder')}</button>
            </div>
          {/each}

          {#if files.length}
            <button class="w-full flex items-center gap-1.5 text-[11.5px] text-muted hover:text-white py-1 cursor-pointer" onclick={() => (showFiles = !showFiles)}>
              <Icon name="chevron" size={11} class="transition-transform {showFiles ? 'rotate-90' : ''}" />
              {t('All files')} ({report.done})
            </button>
            {#if showFiles}
              <ul class="rounded-[10px] border border-line divide-y divide-line/60 overflow-hidden">
                {#each files as f, i (f.path + i)}
                  {@const r = result(f)}
                  <li class="px-3 py-1.5 flex items-center gap-3 text-[11.5px]" title={cloudReason(f.detail) || how(f)}>
                    <span class="mono truncate flex-1 min-w-0">{f.name}</span>
                    <span class="text-dim flex-shrink-0">{bytes(f.size)}</span>
                    <span class="px-1.5 py-[1px] rounded font-medium flex-shrink-0 {r.cls}">{r.text}</span>
                  </li>
                {/each}
              </ul>
              {#if report.done > files.length}
                <p class="text-[11px] text-dim py-1">{t('Only the last {count} files are shown.', { count: files.length })}</p>
              {/if}
            {/if}
          {:else if !report.running}
            <p class="text-[12px] text-muted py-1">{t('No file was checked.')}</p>
          {/if}
        </div>
      {/if}

      <div class="flex justify-end gap-2 px-5 py-3 border-t border-line">
        {#if report?.running}
          <button class="btn btn-danger" disabled={stopping === report.id} onclick={stop}>{stopping === report.id ? t('Stopping…') : t('Stop scan')}</button>
          <button class="btn btn-accent" onclick={close}>{t('Keep scanning in the background')}</button>
        {:else}
          <button class="btn btn-accent" onclick={close}>{t('Close')}</button>
        {/if}
      </div>
    </div>
  </div>
{/if}
