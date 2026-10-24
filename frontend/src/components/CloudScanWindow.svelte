<script lang="ts">
  import { onDestroy } from 'svelte';
  import { app } from '../lib/store.svelte';
  import { api } from '../lib/api';
  import { t } from '../lib/i18n.svelte';
  import { bytes, duration } from '../lib/format';
  import type { CloudFinding } from '../types';
  import Icon from './Icon.svelte';

  let tab = $state<'files' | 'threats'>('files');
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
        return { text: t('Not uploaded'), cls: 'bg-line text-muted' };
      default:
        return { text: t('Error'), cls: 'bg-line text-muted' };
    }
  }

  function how(f: CloudFinding) {
    if (f.route === 'hash') return t('Known fingerprint');
    if (f.route === 'upload') return t('Uploaded and analyzed');
    if (f.route === 'local') return f.verdict === 'skipped' ? t('Checked by fingerprint only') : t('Checked on this PC');
    return '—';
  }
</script>

{#if app.cloudReportFor}
  <div class="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4" role="presentation">
    <div class="w-full max-w-[860px] h-[86vh] max-h-[720px] bg-menu border border-line rounded-[14px] shadow-2xl flex flex-col overflow-hidden">
      <div class="flex items-start gap-3 px-5 py-4 border-b border-line">
        <div class="w-[38px] h-[38px] rounded-full flex items-center justify-center flex-shrink-0 {report?.threats ? 'bg-bad/20 text-bad' : report?.suspicious ? 'bg-warn/20 text-warn' : 'bg-accent/20 text-accent'}">
          <Icon name="shield" size={19} />
        </div>
        <div class="min-w-0 flex-1">
          <h2 class="text-[15px] font-semibold flex items-center gap-2">
            {t('Multron Cloud scan')}
            <span class="px-1.5 rounded bg-accent/15 text-accent text-[10px] font-semibold uppercase tracking-wide">{t('Cloud')}</span>
            <span class="px-1.5 rounded bg-warn/15 text-warn text-[10px] font-semibold uppercase tracking-wide">{t('Testing')}</span>
          </h2>
          <div class="text-[12px] text-muted truncate" title={report?.name ?? ''}>
            {report?.id === 'all' ? t('All downloads') + (report.ids.length ? ' · ' + t('{count} downloads', { count: report.ids.length }) : '') : (report?.name ?? '')}
          </div>
        </div>
        <button class="icon-btn" title={t('Close')} onclick={close}><Icon name="x" size={14} /></button>
      </div>

      {#if !report}
        <div class="p-6 text-[12.5px] text-muted">{t('Starting the scan…')}</div>
      {:else}
        <div class="px-5 pt-4 pb-3 space-y-3 border-b border-line">
          <div>
            <div class="flex justify-between items-baseline text-[12.5px] mb-1.5">
              <span class="font-semibold">
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
                {:else}
                  {t('No threat found')}
                {/if}
              </span>
              <span class="text-muted">{percent}%</span>
            </div>
            <div class="h-[8px] rounded-full bg-line overflow-hidden">
              <div class="h-full rounded-full transition-all {report.threats ? 'bg-bad' : report.suspicious ? 'bg-warn' : report.running ? 'bg-accent' : 'bg-ok'}" style="width: {percent}%"></div>
            </div>
            {#if !report.running && report.state === 'error' && report.summary}
              <div class="text-[11.5px] text-bad mt-1.5 select-text">{report.summary}</div>
            {/if}
          </div>

          <div class="grid grid-cols-4 gap-2 text-[11.5px]">
            <div><div class="text-muted">{t('Elapsed')}</div><div class="font-semibold text-[13px]">{duration(elapsed)}</div></div>
            <div><div class="text-muted">{t('Remaining')}</div><div class="font-semibold text-[13px]">{remaining == null ? '—' : duration(remaining)}</div></div>
            <div><div class="text-muted">{t('Uploaded to the cloud')}</div><div class="font-semibold text-[13px]">{report.uploaded} · {bytes(report.uploadedBytes)}</div></div>
            <div><div class="text-muted">{t('Known by fingerprint')}</div><div class="font-semibold text-[13px]">{report.known}</div></div>
          </div>

          {#if report.running}
            <div class="rounded-[10px] border border-accent/40 bg-accent/5 p-2.5">
              <div class="text-[12px] mono truncate font-medium" title={report.current ?? ''}>{report.stage === 'checking' ? t('Fingerprint check') : (report.current ?? '…')}</div>
              <div class="text-[11.5px] text-accent flex items-center gap-1.5 mt-0.5">
                <Icon name={report.stage === 'uploading' ? 'up' : report.stage === 'hashing' ? 'hash' : 'shield'} size={12} />
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
              </div>
              {#if report.stage === 'uploading'}
                <div class="h-[4px] rounded-full bg-line overflow-hidden mt-1.5"><div class="h-full bg-accent transition-all" style="width: {uploadPercent}%"></div></div>
              {/if}
            </div>
          {/if}

          <div class="grid grid-cols-5 gap-2">
            <div class="rounded-[10px] border border-line px-2.5 py-1.5"><div class="text-[17px] font-semibold text-ok">{report.clean}</div><div class="text-[11px] text-muted">{t('Clean')}</div></div>
            <div class="rounded-[10px] border border-line px-2.5 py-1.5" title={t('No threat was found, but the file is not verified as clean yet')}><div class="text-[17px] font-semibold text-ok/80">{report.unknown ?? 0}</div><div class="text-[11px] text-muted">{t('No detection')}</div></div>
            <div class="rounded-[10px] border px-2.5 py-1.5 {report.threats ? 'border-bad/50 bg-bad/10' : 'border-line'}"><div class="text-[17px] font-semibold text-bad">{report.threats}</div><div class="text-[11px] text-muted">{t('Threats')}</div></div>
            <div class="rounded-[10px] border px-2.5 py-1.5 {report.suspicious ? 'border-warn/50 bg-warn/10' : 'border-line'}"><div class="text-[17px] font-semibold text-warn">{report.suspicious}</div><div class="text-[11px] text-muted">{t('Suspicious')}</div></div>
            <div class="rounded-[10px] border border-line px-2.5 py-1.5"><div class="text-[17px] font-semibold text-muted">{report.notChecked}</div><div class="text-[11px] text-muted">{t('Not checked')}</div></div>
          </div>
        </div>

        <div class="flex gap-1 px-5 pt-2">
          <button class="px-3 h-[28px] rounded-full text-[12px] font-medium cursor-pointer {tab === 'files' ? 'bg-accent-strong/25 text-accent' : 'text-muted hover:bg-line'}" onclick={() => (tab = 'files')}>{t('All files')} ({report.done})</button>
          <button class="px-3 h-[28px] rounded-full text-[12px] font-medium cursor-pointer {tab === 'threats' ? 'bg-bad/15 text-bad' : threats.length ? 'text-bad hover:bg-bad/10' : 'text-muted hover:bg-line'}" onclick={() => (tab = 'threats')}>{t('Threats')} ({threats.length})</button>
        </div>

        <div class="flex-1 min-h-0 overflow-y-auto px-5 py-2">
          {#if tab === 'threats'}
            {#if threats.length}
              <ul class="space-y-1.5">
                {#each threats as f (f.path)}
                  <li class="rounded-[10px] border p-2.5 flex items-start gap-2.5 {f.verdict === 'malicious' ? 'border-bad/50 bg-bad/10' : 'border-warn/50 bg-warn/10'}">
                    <Icon name="alert" size={14} class="mt-[2px] flex-shrink-0 {f.verdict === 'malicious' ? 'text-bad' : 'text-warn'}" />
                    <div class="min-w-0 flex-1">
                      <div class="text-[12px] mono truncate" title={f.name}>{f.name}</div>
                      <div class="text-[11.5px] {f.verdict === 'malicious' ? 'text-bad' : 'text-warn'} select-text">{f.verdict === 'malicious' ? t('Threat found') : t('Suspicious file')}{f.detail ? ' · ' + f.detail : ''}</div>
                    </div>
                    <button class="btn !h-[26px] !text-[11.5px] flex-shrink-0" onclick={() => reveal(f)}><Icon name="folderOpen" size={12} />{t('Show in folder')}</button>
                  </li>
                {/each}
              </ul>
            {:else}
              <p class="text-[12px] text-muted py-2">{report.running ? t('No threats so far.') : t('No threats were found.')}</p>
            {/if}
          {:else if files.length}
            <table class="w-full text-[11.5px] table-fixed">
              <thead class="text-muted text-left sticky top-0 bg-menu">
                <tr>
                  <th class="font-medium py-1">{t('File')}</th>
                  <th class="font-medium py-1 w-[80px] text-right pr-3">{t('Size')}</th>
                  <th class="font-medium py-1 w-[120px]">{t('Result')}</th>
                  <th class="font-medium py-1 w-[170px]">{t('How')}</th>
                </tr>
              </thead>
              <tbody>
                {#each files as f, i (f.path + i)}
                  {@const r = result(f)}
                  <tr class="border-t border-line/60 {f.verdict === 'malicious' ? 'bg-bad/5' : ''}">
                    <td class="py-1 pr-2 mono truncate" title={f.detail ? f.name + ' — ' + f.detail : f.name}>{f.name}</td>
                    <td class="py-1 text-right pr-3 text-muted">{bytes(f.size)}</td>
                    <td class="py-1"><span class="px-1.5 py-[1px] rounded font-medium {r.cls}" title={f.detail}>{r.text}</span></td>
                    <td class="py-1 text-muted truncate" title={f.detail}>{how(f)}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
            {#if report.done > files.length}
              <p class="text-[11px] text-muted py-1.5">{t('Only the last {count} files are shown.', { count: files.length })}</p>
            {/if}
          {:else}
            <p class="text-[12px] text-muted py-2">{report.running ? t('Starting the scan…') : t('No file was checked.')}</p>
          {/if}
        </div>
      {/if}

      <div class="flex justify-end gap-2 px-4 py-3 border-t border-line">
        {#if report?.running}
          <button class="btn btn-danger" disabled={stopping === report.id} onclick={stop}>{stopping === report.id ? t('Stopping…') : t('Stop scan')}</button>
          <button class="btn" onclick={close}>{t('Keep scanning in the background')}</button>
        {:else}
          <button class="btn btn-accent" onclick={close}>{t('Close')}</button>
        {/if}
      </div>
    </div>
  </div>
{/if}
