import { listen } from '@tauri-apps/api/event';
import { api, errorText } from './api';
import { setLanguage, t } from './i18n.svelte';
import { message } from '@tauri-apps/plugin-dialog';
import { applyAppearance, resolveTheme, type Appearance } from './theme';
import type {
  Duplicate,
  CloudReport,
  AddRequest,
  AppUpdate,
  CategoryFilter,
  Download,
  GlobalStats,
  PageBatch,
  ReadyUpdate,
  MandatoryUpdate,
  KindFilter,
  Settings,
  StatusFilter,
  ToolProgress
} from '../types';

export interface Notice {
  id: number;
  kind: 'started' | 'finished' | 'failed' | 'scan';
  downloadId: string;
  name: string;
  engine?: 'cloud';
  result?: string;
  detail?: string | null;
}

import { PROGRAM_FILE } from './files';
export { PROGRAM_FILE };

export interface Toast {
  id: number;
  kind: 'info' | 'success' | 'warning' | 'error';
  message: string;
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

  view = $state<'downloads' | 'settings' | 'rss'>('downloads');
  createTorrentOpen = $state(false);
  settingsSection = $state<string>('general');
  addOpen = $state(false);
  addPrefill = $state<AddRequest | null>(null);
  addTorrentFile = $state<string | null>(null);
  pairOrigins = $state<string[]>([]);
  extensionSetupOpen = $state(false);
  extensionSetupBrowser = $state<string | null>(null);
  batch = $state<PageBatch | null>(null);
  appearancePreview = $state<Appearance | null>(null);
  materials = $state<string[]>([]);
  removeTargets = $state<Download[]>([]);
  cloudEulaFor = $state<string | null>(null);
  cloudPasswordFor = $state<string | null>(null);
  startTarget = $state<Download | null>(null);
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
  appUpdate = $state<AppUpdate | null>(null);
  updateReady = $state<ReadyUpdate | null>(null);
  mandatoryUpdate = $state<MandatoryUpdate | null>(null);
  updateBusy = $state(false);
  disk = $state<{ free: number; total: number } | null>(null);
  itemSpeeds = $state<Record<string, { down: number[]; up: number[] }>>({});
  logs = $state<Record<string, string[]>>({});

  private toastSeq = 0;

  filtered = $derived.by(() => {
    const q = this.search.trim().toLowerCase();
    const list = this.downloads.filter((d) => {
      if (this.statusFilter !== 'all' && d.status !== this.statusFilter) return false;
      if (this.kindFilter !== 'all' && d.kind !== this.kindFilter) return false;
      if (this.categoryFilter !== 'all' && d.category !== this.categoryFilter) return false;
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
      c['cat:' + d.category] = (c['cat:' + d.category] ?? 0) + 1;
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
    await listen<{ id: string | null; start: boolean }>('app://select', (e) => {
      this.view = 'downloads';
      const d = e.payload.id ? this.downloads.find((x) => x.id === e.payload.id) : undefined;
      if (!d) return;
      this.selectedId = d.id;
      if (e.payload.start) this.confirmStart(d);
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
      this.toolProgress = e.payload.done ? null : e.payload;
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
      if (this.settings?.downloadNotices !== false) {
        if (o.status !== d.status && d.status === 'completed') this.notice('finished', d);
        if (o.status !== d.status && d.status === 'failed') this.notice('failed', d);
      }
      if (this.settings?.scanNotices !== false) {
        const inBulk = this.cloudReports.all?.ids.includes(d.id) ?? false;
        if (!inBulk && o.cloudState !== d.cloudState && d.cloudState && d.cloudState !== 'scanning' && d.cloudState !== 'locked') {
          this.notice('scan', d, 'cloud', d.cloudState, d.cloudDetail);
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
      if (d.cloudState === 'locked' && o.cloudState === 'scanning' && !this.cloudPasswordFor && !this.cloudEulaFor) {
        this.cloudPasswordFor = d.id;
      }
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
      else if (p.type === 'batch') this.batch = p.batch;
      else if (p.type === 'openTorrent') this.openAdd({ url: '', kind: 'torrent' }, p.path);
      else if (p.type === 'pair' && !this.pairOrigins.includes(p.origin)) {
        this.pairOrigins = [...this.pairOrigins, p.origin];
      }
    }
  }

  openAdd(prefill: AddRequest | null = null, torrentFile: string | null = null) {
    this.addPrefill = prefill;
    this.addTorrentFile = torrentFile;
    this.addOpen = true;
  }

  notice(kind: Notice['kind'], d: Download, engine?: Notice['engine'], result?: string, detail?: string | null) {
    if (!this.settings?.inAppNotices) return;
    const id = ++this.toastSeq;
    const same = (n: Notice) =>
      n.downloadId === d.id && (kind === 'scan' ? n.kind === 'scan' && n.engine === engine : n.kind !== 'scan');
    this.notices = [
      ...this.notices.filter((n) => !same(n)).slice(-3),
      { id, kind, downloadId: d.id, name: d.name || d.url, engine, result, detail }
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

  async addChecked(request: AddRequest) {
    const existing = await api.findDuplicate(request).catch(() => null);
    if (existing && !(await this.askDuplicate(request, existing))) return false;
    return this.run(() => api.add({ ...request, allowDuplicate: !!existing }));
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
    if (PROGRAM_FILE.test(d.name) && this.settings?.askBeforeStartingPrograms !== false) {
      this.confirmStart(d);
      return;
    }
    api.openFile(d.id).catch((e) => {
      if (String(e).includes('confirm_start')) this.confirmStart(d);
      else this.toast('error', errorText(e));
    });
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
