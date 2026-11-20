import { api, errorText } from './api';
import { app } from './store.svelte';
import type { BatchItem, BrowserCookie, PageBatch } from '../types';

export type LinkStatus = 'checking' | 'online' | 'offline' | 'unknown';
export type FileType = 'video' | 'audio' | 'archive' | 'document' | 'image' | 'program' | 'page' | 'web' | 'other';
export type Priority = 'normal' | 'high' | 'low';

export interface LinkOptions {
  note: string;
  extractPassword: string;
  autoExtract: boolean | null;
  loginUser: string;
  loginPassword: string;
  priority: Priority;
}

export interface GrabberPrefs {
  scanPages: boolean;
  scanSubpages: boolean;
  autoCheck: boolean;
  showOffline: boolean;
  sameSite: boolean;
  hiddenTypes: FileType[];
  optionsOpen: boolean;
  checkTypes: FileType[];
  parallel: number;
  pageTimeout: number;
  subpageLimit: number;
}

export interface GrabLink {
  id: number;
  url: string;
  name: string;
  size: number | null;
  status: LinkStatus;
  host: string;
  pkg: string;
  source: string;
  kind: string;
  referrer: string | null;
  cookies: BrowserCookie[];
  userAgent: string | null;
  error: string | null;
  tag?: string;
  text?: string | null;
  page?: string | null;
  opts?: LinkOptions | null;
}

const STORE_KEY = 'mdm-linkgrabber';
const PREFS_KEY = 'mdm-linkgrabber-prefs';
export const DEFAULT_CHECK_TYPES: FileType[] = ['video', 'audio', 'archive', 'document', 'program', 'other'];
export const SPEED_DEFAULTS = { parallel: 8, pageTimeout: 25, subpageLimit: 25 };
const MAX_LINKS = 5000;

const TYPES: [FileType, RegExp][] = [
  ['archive', /\.(zip|rar|7z|tar|gz|tgz|bz2|xz|zst|iso|r\d{2}|\d{3})$/i],
  ['video', /\.(mp4|mkv|avi|mov|wmv|webm|flv|m4v|mpg|mpeg|ts|m2ts|3gp)$/i],
  ['audio', /\.(mp3|m4a|aac|flac|wav|ogg|opus|wma|alac|ape)$/i],
  ['image', /\.(jpe?g|png|gif|webp|bmp|svg|tiff?|heic|avif|ico)$/i],
  ['document', /\.(pdf|docx?|xlsx?|pptx?|odt|ods|odp|txt|rtf|epub|mobi|azw3|csv|md)$/i],
  ['program', /\.(exe|msi|msix|apk|dmg|pkg|deb|rpm|appimage|bat|cmd|sh|jar)$/i],
  ['web', /\.(css|js|mjs|json|xml|rss|atom|woff2?|ttf|otf|eot|map|wasm|webmanifest)$/i],
  ['page', /\.(html?|xhtml|php\d?|aspx?|jsp|cgi|shtml|cfm)$/i]
];

export function extOf(name: string) {
  const m = name.match(/\.([a-z0-9]{1,8})$/i);
  return m ? m[1].toLowerCase() : '';
}

export function fileType(name: string, url?: string): FileType {
  for (const [type, re] of TYPES) if (re.test(name)) return type;
  if (url && !url.toLowerCase().startsWith('magnet:') && !extOf(name)) {
    try {
      const path = new URL(url).pathname;
      if (path.endsWith('/') || !extOf(path.split('/').pop() ?? '')) return 'page';
    } catch {
      return 'other';
    }
  }
  return 'other';
}

function loadPrefs(): GrabberPrefs {
  const base: GrabberPrefs = { scanPages: true, scanSubpages: false, autoCheck: true, showOffline: true, sameSite: false, hiddenTypes: [], optionsOpen: false, checkTypes: [...DEFAULT_CHECK_TYPES], ...SPEED_DEFAULTS };
  try {
    const p = { ...base, ...(JSON.parse(localStorage.getItem(PREFS_KEY) ?? '{}') as Partial<GrabberPrefs>) };
    const num = (v: unknown, lo: number, hi: number, d: number) => (typeof v === 'number' && Number.isFinite(v) ? Math.min(hi, Math.max(lo, Math.round(v))) : d);
    return {
      ...p,
      checkTypes: Array.isArray(p.checkTypes) ? p.checkTypes : [...DEFAULT_CHECK_TYPES],
      parallel: num(p.parallel, 1, 32, SPEED_DEFAULTS.parallel),
      pageTimeout: num(p.pageTimeout, 3, 300, SPEED_DEFAULTS.pageTimeout),
      subpageLimit: num(p.subpageLimit, 1, 200, SPEED_DEFAULTS.subpageLimit)
    };
  } catch {
    return base;
  }
}

export function hostOf(url: string) {
  if (url.toLowerCase().startsWith('magnet:')) return 'magnet';
  try {
    return new URL(url).hostname.replace(/^www\./, '');
  } catch {
    return '';
  }
}

function nameOf(url: string) {
  if (url.toLowerCase().startsWith('magnet:')) {
    const dn = url.match(/[?&]dn=([^&]+)/i);
    try {
      return dn ? decodeURIComponent(dn[1].replace(/\+/g, ' ')) : 'magnet';
    } catch {
      return 'magnet';
    }
  }
  try {
    const last = new URL(url).pathname.split('/').filter(Boolean).pop();
    return last ? decodeURIComponent(last) : hostOf(url);
  } catch {
    return url;
  }
}

export function packageOf(name: string, source: string) {
  const m = name.match(/^(.*?)(\.part\d+\.rar|\.r\d{2}|\.(?:7z|zip)\.\d{3}|\.rar|\.zip|\.7z)$/i);
  if (m && m[1]) return m[1];
  if (source) return source;
  return name.replace(/\.[^./]{1,5}$/, '') || name;
}

function statusOf(error: string | null, total: number | null, filename: string | null, contentType: string | null, service: string | null): LinkStatus {
  if (error) return /\b(404|410|451)\b|not found|gone|no longer|removed|offline|does not exist/i.test(error) ? 'offline' : 'unknown';
  if (contentType?.startsWith('text/html') && !service) return 'unknown';
  return total != null || filename ? 'online' : 'unknown';
}

class Grabber {
  links = $state<GrabLink[]>([]);
  prefs = $state<GrabberPrefs>(loadPrefs());
  scanning = $state(0);
  scanned = $state(0);
  checkTotal = $state(0);
  checkDone = $state(0);
  addOpen = $state(false);
  private seq = 0;
  private queue: number[] = [];
  private running = 0;
  private gen = 0;
  private pending = new Map<number, Partial<GrabLink>>();
  private flushTimer: ReturnType<typeof setTimeout> | null = null;
  private saveTimer: ReturnType<typeof setTimeout> | null = null;

  get busy() {
    return this.scanning > 0 || this.checkDone < this.checkTotal;
  }

  cancel() {
    this.gen++;
    this.queue = [];
    this.flush();
    this.links = this.links.map((l) => (l.status === 'checking' ? { ...l, status: 'unknown' } : l));
    this.scanning = 0;
    this.scanned = 0;
    this.checkTotal = 0;
    this.checkDone = 0;
    this.saveSoon();
  }

  constructor() {
    try {
      const saved = JSON.parse(localStorage.getItem(STORE_KEY) ?? '[]') as GrabLink[];
      if (Array.isArray(saved)) {
        this.links = saved.map((l) => ({ ...l, cookies: [], status: l.status === 'checking' ? 'unknown' : l.status }));
        this.seq = Math.max(0, ...this.links.map((l) => l.id));
      }
    } catch {
      this.links = [];
    }
  }

  savePrefs() {
    try {
      localStorage.setItem(PREFS_KEY, JSON.stringify(this.prefs));
    } catch {
      return;
    }
  }

  private saveSoon() {
    if (this.saveTimer) return;
    this.saveTimer = setTimeout(() => {
      this.saveTimer = null;
      this.save();
    }, 1500);
  }

  private save() {
    if (app.settings?.privacyForgetLinkgrabber) return;
    try {
      localStorage.setItem(STORE_KEY, JSON.stringify(this.links.map((l) => ({ ...l, cookies: [], opts: l.opts ? { ...l.opts, loginPassword: '', extractPassword: '' } : null }))));
    } catch {
      return;
    }
  }

  add(items: (BatchItem & { tag?: string; text?: string | null })[], batch?: Partial<PageBatch>, extra?: { pkg?: string; opts?: LinkOptions | null }) {
    const known = new Set(this.links.map((l) => l.url));
    const source = extra?.pkg || (items.length > 1 ? batch?.pageTitle || (batch?.pageUrl ? hostOf(batch.pageUrl) : '') : '');
    const check = this.prefs.autoCheck;
    const added: GrabLink[] = [];
    for (const item of items) {
      const url = item.url.trim();
      if (!url || known.has(url) || this.links.length + added.length >= MAX_LINKS) continue;
      known.add(url);
      const name = item.filename || item.name || nameOf(url);
      const magnet = url.toLowerCase().startsWith('magnet:');
      const skip = magnet || !check || !this.prefs.checkTypes.includes(fileType(name, url));
      added.push({
        id: ++this.seq,
        url,
        name,
        size: null,
        status: skip ? 'unknown' : 'checking',
        host: hostOf(url),
        pkg: extra?.pkg || packageOf(name, source),
        source,
        kind: magnet ? 'torrent' : item.kind || 'auto',
        referrer: item.referrer ?? batch?.pageUrl ?? null,
        cookies: batch?.cookies ?? [],
        userAgent: batch?.userAgent ?? null,
        error: null,
        tag: item.tag,
        text: item.text ?? null,
        page: batch?.pageUrl ?? null,
        opts: extra?.opts ?? null
      });
    }
    if (!added.length) return 0;
    this.links = [...this.links, ...added];
    this.saveSoon();
    this.enqueue(added.filter((l) => l.status === 'checking').map((l) => l.id));
    return added.length;
  }

  recheck(ids?: number[]) {
    const wanted = new Set(ids ?? this.links.map((l) => l.id));
    this.flush();
    const queued = new Set(this.queue);
    const fresh: number[] = [];
    this.links = this.links.map((l) => {
      if (!wanted.has(l.id) || l.kind === 'torrent') return l;
      if (!queued.has(l.id)) fresh.push(l.id);
      return { ...l, status: 'checking', error: null };
    });
    this.enqueue(fresh);
  }

  private enqueue(ids: number[]) {
    if (!ids.length) return;
    if (!this.running && !this.queue.length) {
      this.checkTotal = 0;
      this.checkDone = 0;
    }
    this.queue.push(...ids);
    this.checkTotal += ids.length;
    this.pump();
  }

  private update(id: number, change: Partial<GrabLink>) {
    this.pending.set(id, { ...(this.pending.get(id) ?? {}), ...change });
    if (!this.flushTimer) this.flushTimer = setTimeout(() => this.flush(), 250);
  }

  private flush() {
    if (this.flushTimer) clearTimeout(this.flushTimer);
    this.flushTimer = null;
    if (!this.pending.size) return;
    const changes = this.pending;
    this.pending = new Map();
    this.links = this.links.map((l) => {
      const c = changes.get(l.id);
      return c ? { ...l, ...c } : l;
    });
  }

  private pump() {
    const gen = this.gen;
    while (this.running < this.prefs.parallel && this.queue.length) {
      const id = this.queue.shift()!;
      const link = this.links.find((l) => l.id === id);
      if (!link) {
        this.checkDone++;
        continue;
      }
      this.running++;
      api
        .checkLink(link.url, link.referrer, link.cookies)
        .then((p) => {
          if (gen !== this.gen) return;
          const name = p.filename || link.name;
          const fixed = !!link.opts && !!link.source && link.pkg === link.source;
          this.update(id, {
            name,
            size: p.total,
            status: statusOf(p.error, p.total, p.filename, p.contentType, p.service),
            pkg: fixed ? link.pkg : packageOf(name, link.source),
            error: p.error
          });
        })
        .catch((e) => gen === this.gen && this.update(id, { status: 'unknown', error: errorText(e) }))
        .finally(() => {
          this.running--;
          if (gen === this.gen) this.checkDone++;
          if (!this.running && !this.queue.length) this.flush();
          this.saveSoon();
          this.pump();
        });
    }
  }

  remove(ids: number[]) {
    const gone = new Set(ids);
    this.flush();
    const before = this.queue.length;
    this.links = this.links.filter((l) => !gone.has(l.id));
    this.queue = this.queue.filter((id) => !gone.has(id));
    this.checkTotal = Math.max(this.checkDone, this.checkTotal - (before - this.queue.length));
    this.save();
  }

  async analyse(urls: string[], opts: LinkOptions | null, pkg: string) {
    const direct: string[] = [];
    const pages: string[] = [];
    for (const u of urls) (this.prefs.scanPages && /^https?:\/\//i.test(u) ? pages : direct).push(u);
    let added = 0;
    const gen = this.gen;
    if (!this.scanning) this.scanned = 0;
    if (direct.length) added += this.add(direct.map((url) => ({ url, kind: 'auto', mediaType: 'file' })), undefined, { pkg, opts });
    const visited = new Set<string>();
    const scanOne = async (url: string, depth: number) => {
      if (gen !== this.gen || visited.has(url) || visited.size > this.prefs.subpageLimit + urls.length) return;
      visited.add(url);
      this.scanning++;
      try {
        const scan = await api.scanPage(url, this.prefs.pageTimeout);
        if (gen !== this.gen) return;
        if (scan.isFile || !scan.links.length) {
          added += this.add([{ url, kind: 'auto', mediaType: 'file' }], undefined, { pkg, opts });
          return;
        }
        const title = pkg || scan.title || hostOf(scan.url) || url;
        added += this.add(
          scan.links.map((l) => ({ url: l.url, kind: 'auto', mediaType: 'file', tag: l.tag, text: l.text })),
          { pageUrl: scan.url, pageTitle: title },
          { pkg: title, opts }
        );
        if (depth === 0 && this.prefs.scanSubpages) {
          const host = hostOf(scan.url);
          const subs = scan.links
            .filter((l) => l.tag === 'a' && hostOf(l.url) === host && fileType(nameOf(l.url), l.url) === 'page')
            .slice(0, this.prefs.subpageLimit);
          for (let i = 0; i < subs.length && gen === this.gen; i += 4) await Promise.all(subs.slice(i, i + 4).map((s) => scanOne(s.url, 1)));
        }
      } catch (e) {
        if (depth === 0 && gen === this.gen) {
          added += this.add([{ url, kind: 'auto', mediaType: 'file' }], undefined, { pkg, opts });
          app.toast('error', hostOf(url) + ': ' + errorText(e));
        }
      } finally {
        if (gen === this.gen) {
          this.scanning--;
          this.scanned++;
        }
      }
    };
    await Promise.all(pages.map((p) => scanOne(p, 0)));
    return added;
  }

  async start(ids: number[], dir: string | null, paused: boolean) {
    const chosen = this.links.filter((l) => ids.includes(l.id));
    const started: number[] = [];
    let failed = 0;
    for (const l of chosen) {
      try {
        const o = l.opts;
        const id = await api.add({
          url: l.url,
          kind: l.kind === 'torrent' ? 'torrent' : l.kind === 'media' ? 'media' : null,
          dir,
          filename: l.status === 'online' && l.kind !== 'torrent' ? l.name : null,
          referrer: l.referrer,
          userAgent: l.userAgent,
          cookies: l.cookies,
          startPaused: paused,
          note: o?.note || undefined,
          autoExtract: o?.autoExtract ?? null,
          extractPassword: o?.extractPassword || null,
          loginUser: o?.loginPassword ? o.loginUser || '' : null,
          loginPassword: o?.loginPassword || null
        });
        if (id && o?.priority === 'high') await api.moveInQueue(id, 'top').catch(() => {});
        if (id && o?.priority === 'low') await api.moveInQueue(id, 'bottom').catch(() => {});
        started.push(l.id);
      } catch (e) {
        failed++;
        this.update(l.id, { error: errorText(e) });
      }
    }
    this.remove(started);
    if (failed) app.toast('error', `${failed} / ${chosen.length}`);
    return started.length;
  }
}

export const grabber = new Grabber();
