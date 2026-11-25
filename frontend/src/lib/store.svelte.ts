import { listen } from '@tauri-apps/api/event';
import { api, errorText } from './api';
import { setLanguage, t } from './i18n.svelte';
import { ask, message } from '@tauri-apps/plugin-dialog';
import { applyAppearance, resolveTheme, type Appearance } from './theme';
import type { VtResult, CaptchaRequest, Duplicate, CloudReport, AddRequest, AppUpdate, CategoryFilter, Download, GlobalStats, PageBatch, ReadyUpdate, MandatoryUpdate, KindFilter, Settings, StatusFilter, ToolProgress, CategoryLanguage } from '../types';

export interface Notice {
  id: number;
  kind: 'started' | 'finished' | 'failed' | 'scan';
  downloadId: string;
  name: string;
  engine?: 'cloud';
  result?: string;
  detail?: string | null;
}

import { DISGUISED_NAME, PROGRAM_FILE } from './files';
import { grabber } from './grabber.svelte';
import { linkBatch } from './links';
export { DISGUISED_NAME, PROGRAM_FILE };

export interface Toast {
  id: number;
  kind: 'info' | 'success' | 'warning' | 'error';
  message: string;
}

export function doneSound(failed = false) {
  try {
    const ctx = new AudioContext();
    const notes = failed ? [523.25, 392] : [659.25, 783.99, 1046.5];
    notes.forEach((freq, i) => {
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.type = failed ? 'triangle' : 'sine';
      osc.frequency.value = freq;
      const at = ctx.currentTime + i * 0.13;
      gain.gain.setValueAtTime(0.0001, at);
      gain.gain.exponentialRampToValueAtTime(0.16, at + 0.02);
      gain.gain.exponentialRampToValueAtTime(0.0001, at + (i === notes.length - 1 ? 0.6 : 0.25));
      osc.connect(gain).connect(ctx.destination);
      osc.start(at);
      osc.stop(at + 0.65);
    });
    setTimeout(() => ctx.close().catch(() => {}), 1500);
  } catch {
    return;
  }
}

function chime() {
  try {
    const ctx = new AudioContext();
    const notes = [880, 1320];
    notes.forEach((freq, i) => {
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.type = 'sine';
      osc.frequency.value = freq;
      const at = ctx.currentTime + i * 0.16;
      gain.gain.setValueAtTime(0.0001, at);
      gain.gain.exponentialRampToValueAtTime(0.18, at + 0.02);
      gain.gain.exponentialRampToValueAtTime(0.0001, at + 0.28);
      osc.connect(gain).connect(ctx.destination);
      osc.start(at);
      osc.stop(at + 0.3);
    });
    setTimeout(() => ctx.close().catch(() => {}), 900);
  } catch {
    return;
  }
}

class AppState {
  ready = $state(false);
  version = $state('');
  platform = $state('');
  dataDir = $state('');
  downloads = $state<Download[]>([]);
  settings = $state<Settings | null>(null);
  stats = $state<GlobalStats>({ downloadSpeed: 0, uploadSpeed: 0, active: 0, queued: 0, torrent: null, altSpeed: false });
  speedHistory = $state<number[]>([]);
  uploadHistory = $state<number[]>([]);

  statusFilter = $state<StatusFilter>('all');
  kindFilter = $state<KindFilter>('all');
  categoryFilter = $state<CategoryFilter>('all');
  search = $state('');
  selectedId = $state<string | null>(null);

  view = $state<'downloads' | 'settings' | 'rss' | 'search' | 'grabber' | 'stats' | 'history'>('downloads');
  tagFilter = $state<string | null>(null);
  paletteOpen = $state(false);
  createTorrentOpen = $state(false);
  settingsSection = $state<string>('general');
  addOpen = $state(false);
  addPrefill = $state<AddRequest | null>(null);
  addTorrentFile = $state<string | null>(null);
  addDone: ((id: string) => void) | null = null;
  pairOrigins = $state<string[]>([]);
  extensionSetupOpen = $state(false);
  extensionSetupBrowser = $state<string | null>(null);
  batch = $state<PageBatch | null>(null);
  categoryLanguage = $state<CategoryLanguage | null>(null);
  grabbed = $state<string[]>([]);
  captchas = $state<CaptchaRequest[]>([]);
  captchaPanel = $state(false);
  private captchaTimer: ReturnType<typeof setInterval> | undefined;
  appearancePreview = $state<Appearance | null>(null);
  materials = $state<string[]>([]);
  removeTargets = $state<Download[]>([]);
  cloudEulaFor = $state<string | null>(null);
  cloudPasswordFor = $state<string | null>(null);
  startTarget = $state<Download | null>(null);
  loginFor = $state<Download | null>(null);
  passwordFor = $state<Download | null>(null);
  categoryDialog = $state<{ index: number | null } | null>(null);
  vtResult = $state<VtResult | null>(null);
  vtBusy = $state<string | null>(null);
  private openAfterScan = new Set<string>();
  get confirmingOpen() {
    return this.startTarget !== null;
  }
  cloudReports = $state<Record<string, CloudReport>>({});
  cloudReportFor = $state<string | null>(null);
  checked = $state<string[]>([]);
  checkAnchor: string | null = null;
  toasts = $state<Toast[]>([]);
  notices = $state<Notice[]>([]);
  toolProgress = $state<ToolProgress | null>(null);
  appUpdateProgress = $state<ToolProgress | null>(null);
  appUpdate = $state<AppUpdate | null>(null);
  updateReady = $state<ReadyUpdate | null>(null);
  mandatoryUpdate = $state<MandatoryUpdate | null>(null);
  updateBusy = $state(false);
  disk = $state<{ free: number; total: number } | null>(null);
  itemSpeeds = $state<Record<string, { down: number[]; up: number[] }>>({});
  logs = $state<Record<string, string[]>>({});

  private toastSeq = 0;

  categoryOf(d: Download): CategoryFilter {
    const dot = d.name.lastIndexOf('.');
    const ext = dot > 0 ? d.name.slice(dot + 1).toLowerCase() : '';
    const custom = ext ? this.settings?.customCategories.find((c) => c.extensions.includes(ext)) : undefined;
    return custom ? `custom:${custom.name}` : d.category;
  }

  filtered = $derived.by(() => {
    const q = this.search.trim().toLowerCase();
    const list = this.downloads.filter((d) => {
      if (this.statusFilter !== 'all' && d.status !== this.statusFilter) return false;
      if (this.kindFilter !== 'all' && d.kind !== this.kindFilter) return false;
      if (this.categoryFilter !== 'all' && this.categoryOf(d) !== this.categoryFilter) return false;
      if (this.tagFilter && !(d.tags ?? []).includes(this.tagFilter)) return false;
      if (q && !d.name.toLowerCase().includes(q) && !d.url.toLowerCase().includes(q)) return false;
      return true;
    });
    if (this.statusFilter === 'queued') list.sort((a, b) => (a.queuePos ?? 0) - (b.queuePos ?? 0));
    return list;
  });

  counts = $derived.by(() => {
    const c: Record<string, number> = { all: this.downloads.length };
    for (const d of this.downloads) {
      c[d.status] = (c[d.status] ?? 0) + 1;
      c['kind:' + d.kind] = (c['kind:' + d.kind] ?? 0) + 1;
      const cat = 'cat:' + this.categoryOf(d);
      c[cat] = (c[cat] ?? 0) + 1;
    }
    return c;
  });

  selected = $derived(this.downloads.find((d) => d.id === this.selectedId) ?? null);

  checkedItems = $derived(this.downloads.filter((d) => this.checked.includes(d.id)));

  pick(id: string, e: MouseEvent | KeyboardEvent) {
    if (e.shiftKey && this.checkAnchor) {
      const ids = this.filtered.map((d) => d.id);
      const a = ids.indexOf(this.checkAnchor);
      const b = ids.indexOf(id);
      if (a >= 0 && b >= 0) {
        const range = ids.slice(Math.min(a, b), Math.max(a, b) + 1);
        this.checked = [...new Set([...this.checked, ...range])];
        return;
      }
    }
    if (e.ctrlKey || e.metaKey) {
      if (this.checked.length === 0 && this.selectedId && this.selectedId !== id) this.checked = [this.selectedId];
      this.checked = this.checked.includes(id) ? this.checked.filter((x) => x !== id) : [...this.checked, id];
      this.checkAnchor = id;
      return;
    }
    this.checked = [];
    this.checkAnchor = id;
    this.selectedId = this.selectedId === id ? null : id;
  }

  checkAll() {
    this.checked = this.filtered.map((d) => d.id);
  }

  clearChecks() {
    this.checked = [];
  }

  askRemove(items: Download[]) {
    if (items.length) this.removeTargets = items;
  }

  async init() {
    const boot = await api.bootstrap();
    this.version = boot.version;
    this.platform = boot.platform;
    this.dataDir = boot.dataDir;
    this.downloads = boot.downloads;
    this.settings = boot.settings;
    await setLanguage(boot.settings.language);
    if (boot.settings.privacyForgetLinkgrabber) {
      try {
        localStorage.removeItem('mdm-linkgrabber');
      } catch {
        this.grabbed = [];
      }
    }
    await applyAppearance(boot.settings);
    if (boot.settings.cloudScan && !boot.settings.cloudScanAccepted) this.cloudEulaFor = 'launch';
    api
      .appearanceSupport()
      .then((s) => (this.materials = s.materials))
      .catch(() => {});
    for (const d of boot.downloads) this.log(d.id, 'Loaded (' + d.status + ')');

    await listen<Download[]>('downloads://changed', (e) => {
      this.trackChanges(this.downloads, e.payload);
      this.downloads = e.payload;
    });
    await listen<GlobalStats>('stats://global', (e) => {
      this.stats = e.payload;
      this.speedHistory = [...this.speedHistory.slice(-119), e.payload.downloadSpeed];
      this.uploadHistory = [...this.uploadHistory.slice(-119), e.payload.uploadSpeed];
    });
    await listen('app://pending', () => this.drainPending());
    await listen('captcha://changed', () => this.refreshCaptchas());
    this.refreshCaptchas();
    await listen<string[]>('app://clipboard-links', (e) => {
      const next = [...this.grabbed];
      for (const l of e.payload) if (!next.includes(l)) next.push(l);
      this.grabbed = next.slice(-500);
      grabber.add(linkBatch(e.payload, t('Copied links')).items);
    });
    await listen<{ id: string | null; start: boolean; report?: boolean }>('app://select', (e) => {
      this.view = 'downloads';
      const d = e.payload.id ? this.downloads.find((x) => x.id === e.payload.id) : undefined;
      if (!d) return;
      this.selectedId = d.id;
      if (e.payload.report) this.showCloudReport(d);
      else if (e.payload.start) this.confirmStart(d);
    });
    await listen<CloudReport>('cloud://report', (e) => {
      const before = this.cloudReports[e.payload.id];
      this.cloudReports = { ...this.cloudReports, [e.payload.id]: e.payload };
      if (e.payload.id === 'all' && before?.running && !e.payload.running && this.settings?.scanNotices !== false) {
        const all = { id: 'all', name: t('All downloads'), url: '' } as Download;
        this.notice('scan', all, 'cloud', e.payload.state, e.payload.summary);
      }
    });
    await listen<ToolProgress>('tools://progress', (e) => {
      if (e.payload.tool.startsWith('Multron Download Manager')) this.appUpdateProgress = e.payload;
      else this.toolProgress = e.payload.done ? null : e.payload;
    });
    await listen<{ kind: Toast['kind']; message: string }>('app://toast', (e) =>
      this.toast(e.payload.kind, e.payload.message)
    );
    await listen<AppUpdate>('app://update', (e) => {
      this.appUpdate = e.payload;
    });
    await listen<MandatoryUpdate | null>('app://mandatory-update', (e) => {
      this.mandatoryUpdate = e.payload;
    });
    api.mandatoryUpdateStatus().then((s) => (this.mandatoryUpdate = s)).catch(() => {});
    await listen<ReadyUpdate>('app://update-ready', (e) => {
      this.updateReady = e.payload;
    });
    api.appUpdateReady().then((r) => (this.updateReady = r)).catch(() => {});
    await this.drainPending();
    if (!boot.settings.extensionSetupDone && boot.settings.trustedExtensions.length === 0) {
      this.extensionSetupOpen = true;
    }
    this.refreshDisk();
    setInterval(() => this.refreshDisk(), 15000);
    this.ready = true;
  }

  private log(id: string, line: string) {
    const stamp = new Date().toLocaleTimeString();
    const list = this.logs[id] ?? [];
    this.logs[id] = [...list.slice(-199), '[' + stamp + '] ' + line];
  }

  private trackChanges(prev: Download[], next: Download[]) {
    const old = new Map(prev.map((d) => [d.id, d]));
    for (const d of next) {
      const s = this.itemSpeeds[d.id] ?? { down: [], up: [] };
      this.itemSpeeds[d.id] = {
        down: [...s.down.slice(-119), d.speed],
        up: [...s.up.slice(-119), d.uploadSpeed]
      };
      const o = old.get(d.id);
      if (!o) {
        if (this.settings?.downloadNotices !== false) this.notice('started', d);
        this.log(d.id, 'Added: ' + (d.url || d.name));
        continue;
      }
      if (o.status !== d.status) this.log(d.id, 'Status: ' + o.status + ' → ' + d.status);
      if (o.status !== d.status && d.status === 'completed' && this.settings?.soundOnComplete !== false) doneSound();
      if (o.status !== d.status && d.status === 'failed' && this.settings?.soundOnFail) doneSound(true);
      if (this.settings?.downloadNotices !== false) {
        if (o.status !== d.status && d.status === 'completed') this.notice('finished', d);
        if (o.status !== d.status && d.status === 'failed') this.notice('failed', d);
      }
      if (this.settings?.scanNotices !== false) {
        const inBulk = this.cloudReports.all?.ids.includes(d.id) ?? false;
        if (!inBulk && o.cloudState !== d.cloudState && d.cloudState && d.cloudState !== 'scanning' && d.cloudState !== 'locked') {
          const report = this.cloudReports[d.id];
          const finished = report && !report.running ? report : null;
          this.notice('scan', d, 'cloud', finished?.state || d.cloudState, finished?.summary ?? d.cloudDetail);
        }
      }
      if (o.phase !== d.phase && d.phase) this.log(d.id, 'Phase: ' + d.phase);
      if (o.error !== d.error && d.error) this.log(d.id, 'Error: ' + d.error);
      if (o.hashState !== d.hashState) {
        this.log(d.id, 'SHA-256: ' + d.hashState + (d.sha256Actual ? ' (' + d.sha256Actual + ')' : ''));
      }
      if (o.threads !== d.threads) this.log(d.id, 'Connections set to ' + d.threads);
      if (o.name !== d.name) this.log(d.id, 'File: ' + d.name);
      if (d.seeding && !o.seeding) this.log(d.id, 'Seeding started');
      if (this.openAfterScan.has(d.id) && o.cloudState === 'scanning' && d.cloudState !== 'scanning') {
        this.openAfterScan.delete(d.id);
        if (d.cloudState !== 'threat' && d.cloudState !== 'suspicious' && !d.quarantined) {
          api.openFile(d.id, false, true).catch((e) => this.handleOpenError(d, String(e)));
        }
      }
      if (d.cloudState === 'locked' && o.cloudState === 'scanning' && !this.cloudPasswordFor && !this.cloudEulaFor) {
        this.cloudPasswordFor = d.id;
      }
    }
  }

  async refreshCaptchas() {
    const before = new Set(this.captchas.map((c) => c.id));
    const list = await api.captchaList().catch(() => [] as CaptchaRequest[]);
    const fresh = list.some((c) => !before.has(c.id));
    this.captchas = list;
    if (fresh) {
      this.captchaPanel = true;
      if (this.settings?.captchaSound !== false) chime();
    }
    if (!list.length) this.captchaPanel = false;
    if (list.length && !this.captchaTimer) this.captchaTimer = setInterval(() => this.refreshCaptchas(), 10000);
    if (!list.length && this.captchaTimer) {
      clearInterval(this.captchaTimer);
      this.captchaTimer = undefined;
    }
  }

  async refreshDisk() {
    try {
      this.disk = await api.diskSpace(this.settings?.downloadDir);
    } catch {
      this.disk = null;
    }
  }

  async drainPending() {
    const items = await api.takePending();
    for (const p of items) {
      if (p.type === 'add') this.openAdd(p.request);
      else if (p.type === 'duplicate') {
        const request = p.request;
        this.askDuplicate(request, p.existing).then((again) => {
          if (again) this.run(() => api.add({ ...request, allowDuplicate: true }));
        });
      }
      else if (p.type === 'batch') {
        if (p.batch.items.length && p.batch.items.every((i) => i.mediaType === 'file')) {
          grabber.add(p.batch.items, p.batch);
          this.view = 'grabber';
        } else this.batch = p.batch;
      }
      else if (p.type === 'categoryLanguage') this.categoryLanguage = { from: p.from, to: p.to, folders: p.folders };
      else if (p.type === 'openTorrent') this.openAdd({ url: '', kind: 'torrent' }, p.path);
      else if (p.type === 'pair' && !this.pairOrigins.includes(p.origin)) {
        this.pairOrigins = [...this.pairOrigins, p.origin];
      }
    }
  }

  openAdd(prefill: AddRequest | null = null, torrentFile: string | null = null, onDone: ((id: string) => void) | null = null) {
    this.addPrefill = prefill;
    this.addTorrentFile = torrentFile;
    this.addDone = onDone;
    this.addOpen = true;
  }

  notice(kind: Notice['kind'], d: Download, engine?: Notice['engine'], result?: string, detail?: string | null) {
    if (!this.settings?.inAppNotices) return;
    const id = ++this.toastSeq;
    const same = (n: Notice) =>
      n.downloadId === d.id && (kind === 'scan' ? n.kind === 'scan' && n.engine === engine : n.kind !== 'scan');
    this.notices = [
      ...this.notices.filter((n) => !same(n)).slice(-3),
      { id, kind, downloadId: d.id, name: this.settings?.privacyHideNames && kind !== 'scan' ? t('A download') : d.name || d.url, engine, result, detail }
    ];
    const bad = result === 'threat' || result === 'suspicious';
    setTimeout(() => this.dismissNotice(id), bad ? 20000 : kind === 'started' ? 6000 : 9000);
  }

  async scanInCloud(d: Download) {
    const ok = await this.run(() => api.cloudScan(d.id));
    if (ok) this.cloudReportFor = d.id;
  }

  async scanAllInCloud() {
    if (!this.settings?.cloudScanAccepted) {
      this.cloudEulaFor = 'all';
      return;
    }
    if (this.cloudReports.all?.running) {
      this.cloudReportFor = 'all';
      return;
    }
    if (await this.run(() => api.cloudScanAll())) this.cloudReportFor = 'all';
  }

  async showCloudReport(d: Download) {
    if (!this.cloudReports[d.id]) {
      const r = await api.cloudScanReport(d.id).catch(() => null);
      if (r) this.cloudReports = { ...this.cloudReports, [d.id]: r };
    }
    if (this.cloudReports[d.id]) {
      this.cloudReportFor = d.id;
      return true;
    }
    return false;
  }

  async askDuplicate(request: AddRequest, existing: Duplicate): Promise<boolean> {
    if (existing.previous) {
      const when = existing.at ? new Date(existing.at).toLocaleString() : '';
      const ok = await message(`${existing.name}\n\n${t('You downloaded this before ({when}). Download it again?', { when })}`, {
        title: t('Downloaded before'),
        kind: 'info',
        buttons: { ok: t('Download again'), cancel: t('Cancel') }
      }).catch(() => 'Cancel');
      return ok === t('Download again') || ok === 'Ok';
    }
    const running = existing.status === 'active' || existing.status === 'queued' || existing.status === 'paused';
    const title = running
      ? t('This file is already downloading')
      : existing.status === 'completed'
        ? t('This file was already downloaded')
        : t('This file is already in the list');
    const again = t('Download again');
    const show = t('Show existing');
    const answer = await message(`${existing.name}\n\n${t('Do you want to download it again?')}`, {
      title,
      kind: 'warning',
      buttons: { yes: again, no: show, cancel: t('Cancel') }
    }).catch(() => 'Cancel');
    if (answer === again || answer === 'Yes') return true;
    if (answer === show || answer === 'No') {
      this.view = 'downloads';
      this.selectedId = existing.id;
    }
    return false;
  }

  async addChecked(request: AddRequest): Promise<string | null> {
    const existing = await api.findDuplicate(request).catch(() => null);
    if (existing && !(await this.askDuplicate(request, existing))) return null;
    try {
      return await api.add({ ...request, allowDuplicate: !!existing });
    } catch (e) {
      this.toast('error', errorText(e));
      return null;
    }
  }

  disguised(d: Download) {
    return !!d.typeMismatch || (this.settings?.warnDisguisedNames !== false && DISGUISED_NAME.test(d.name));
  }

  confirmStart(d: Download) {
    this.startTarget = d;
  }

  async startConfirmed() {
    const d = this.startTarget;
    this.startTarget = null;
    if (d) await this.run(() => api.openFile(d.id, true));
  }

  openDownload(d: Download) {
    if (this.disguised(d) || this.settings?.askBeforeOpeningFiles || (PROGRAM_FILE.test(d.name) && this.settings?.askBeforeStartingPrograms !== false)) {
      this.confirmStart(d);
      return;
    }
    api.openFile(d.id).catch((e) => this.handleOpenError(d, String(e)));
  }

  handleOpenError(d: Download, e: string) {
    if (e.includes('confirm_start') || e.includes('blocked_threat')) this.confirmStart(d);
    else if (e.includes('in_quarantine')) {
      this.toast('warning', t('This file is in quarantine. You can restore it in Settings → Multron Cloud.'));
      this.settingsSection = 'cloud';
      this.view = 'settings';
    } else if (e.includes('rescan_needed')) {
      this.openAfterScan.add(d.id);
      this.toast('info', t('Checking the file with Multron Cloud before it opens…'));
      this.scanInCloud(d);
    } else this.toast('error', errorText(e));
  }

  async virusTotal(d: Download) {
    if (this.vtBusy) return;
    this.vtBusy = d.id;
    try {
      if (!this.settings?.virustotalKey) {
        await api.virustotalOpen(d.id);
        this.toast('info', t('Opened the file’s page on VirusTotal in your browser. Only its fingerprint was sent.'));
        return;
      }
      let r = await api.virustotalCheck(d.id, false);
      if (r.state === 'not_found') {
        const yes = await ask(t('VirusTotal does not know this file yet. Upload it for a scan? Files uploaded to VirusTotal are shared with security companies and can be downloaded by its partners, so do not upload private or personal files.'), {
          title: 'VirusTotal',
          kind: 'warning',
          okLabel: t('Upload'),
          cancelLabel: t('Cancel')
        }).catch(() => false);
        if (!yes) return;
        this.toast('info', t('Uploading to VirusTotal…'));
        r = await api.virustotalCheck(d.id, true);
      }
      this.vtResult = r;
    } catch (e) {
      this.toast('error', errorText(e));
    } finally {
      this.vtBusy = null;
    }
  }

  dismissNotice(id: number) {
    this.notices = this.notices.filter((n) => n.id !== id);
  }

  toast(kind: Toast['kind'], message: string) {
    const id = ++this.toastSeq;
    this.toasts = [...this.toasts, { id, kind, message: t(message) }];
    setTimeout(
      () => {
        this.toasts = this.toasts.filter((t) => t.id !== id);
      },
      kind === 'error' ? 8000 : 4000
    );
  }

  async run(fn: () => Promise<unknown>, success?: string) {
    try {
      await fn();
      if (success) this.toast('success', success);
      return true;
    } catch (e) {
      this.toast('error', errorText(e));
      return false;
    }
  }

  async downloadUpdate() {
    this.updateBusy = true;
    try {
      const r = await api.downloadAppUpdate();
      if (r) this.updateReady = r;
      else this.toast('success', 'Multron Download Manager is up to date');
    } catch (e) {
      this.toast('error', errorText(e));
    } finally {
      this.updateBusy = false;
    }
  }

  async installUpdate() {
    await this.run(() => api.installAppUpdate());
  }

  async toggleAltSpeed() {
    if (!this.settings) return;
    await this.saveSettings({ ...this.settings, altSpeedEnabled: !this.settings.altSpeedEnabled });
  }

  async setWhenDone(whenDone: Settings['whenDone']) {
    if (!this.settings) return;
    await this.saveSettings({ ...this.settings, whenDone });
  }

  async toggleSequential() {
    if (!this.settings) return;
    const on = !this.settings.sequential;
    if (await this.saveSettings({ ...this.settings, sequential: on })) {
      this.toast('info', on ? 'Downloading one at a time, in queue order' : 'Downloading in parallel again');
    }
  }

  async toggleTheme() {
    if (!this.settings) return;
    const next = resolveTheme(this.settings.theme) === 'dark' ? 'light' : 'dark';
    await this.saveSettings({ ...this.settings, theme: next });
  }

  async saveSettings(next: Settings) {
    try {
      this.settings = await api.saveSettings(next);
      await setLanguage(this.settings.language);
      this.refreshDisk();
      return true;
    } catch (e) {
      this.toast('error', errorText(e));
      return false;
    }
  }
}

export const app = new AppState();
