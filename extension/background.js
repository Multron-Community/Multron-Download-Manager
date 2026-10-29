
const DEFAULTS = {
  port: 6801,
  intercept: true,
  minSizeKB: 0,
  sendCookies: true,
  mediaButton: true,
  fullscreenButton: true,
  interceptMagnets: true,
  skipDomains: ''
};

const MEDIA_HOSTS = [
  'youtube.com', 'youtu.be', 'music.youtube.com', 'reddit.com', 'redd.it', 'instagram.com',
  'tiktok.com', 'twitter.com', 'x.com', 'facebook.com', 'fb.watch', 'vimeo.com',
  'dailymotion.com', 'twitch.tv', 'soundcloud.com', 'bilibili.com', 'streamable.com',
  'threads.net', 'kick.com', 'pinterest.com', 'bandcamp.com'
];

const NATIVE_HOST = 'com.multron.downloadmanager';
const ownDownloads = new Set();
let launching = null;
let pingCache = { at: 0, value: null };

async function settings() {
  const stored = await chrome.storage.local.get(DEFAULTS);
  return { ...DEFAULTS, ...stored };
}

function hostOf(url) {
  try {
    return new URL(url).hostname.replace(/^www\./, '').replace(/^m\./, '');
  } catch {
    return '';
  }
}

function isMediaUrl(url) {
  const h = hostOf(url);
  return MEDIA_HOSTS.some((d) => h === d || h.endsWith('.' + d));
}

async function call(path, body, timeoutMs = 4000) {
  const { port } = await settings();
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), timeoutMs);
  try {
    const res = await fetch(`http://127.0.0.1:${port}/api/${path}`, {
      method: body === undefined ? 'GET' : 'POST',
      headers: body === undefined ? {} : { 'Content-Type': 'application/json' },
      body: body === undefined ? undefined : JSON.stringify(body),
      signal: ctrl.signal
    });
    let data = null;
    try {
      data = await res.json();
    } catch {
      data = null;
    }
    return { ok: res.ok, status: res.status, data };
  } finally {
    clearTimeout(timer);
  }
}

async function ping(force = false) {
  if (!force && Date.now() - pingCache.at < 4000) return pingCache.value;
  let value = null;
  try {
    const r = await call('ping', {}, 1500);
    value = r.ok ? r.data : null;
  } catch {
    value = null;
  }
  pingCache = { at: Date.now(), value };
  if (value?.uninstall) {
    removeSelf();
    return null;
  }
  updateBadge(value);
  if (value && !value.paired) autoPair();
  if (value?.version) reloadIfUpdated(value.version);
  if (value?.extensionBuild) reloadIfRebuilt(value.extensionBuild);
  if (typeof value?.fullscreenButton === 'boolean') syncFullscreenButton(value.fullscreenButton);
  return value;
}

async function syncFullscreenButton(on) {
  const { fullscreenButton } = await chrome.storage.local.get({ fullscreenButton: true });
  if (fullscreenButton !== on) await chrome.storage.local.set({ fullscreenButton: on });
}

function versionParts(v) {
  return String(v).split('-')[0].split('.').map((n) => parseInt(n, 10) || 0);
}

function isNewer(a, b) {
  const x = versionParts(a);
  const y = versionParts(b);
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    if ((x[i] ?? 0) !== (y[i] ?? 0)) return (x[i] ?? 0) > (y[i] ?? 0);
  }
  return false;
}

async function ownBuild() {
  const session = chrome.storage.session;
  const saved = session ? (await session.get('loadedBuild')).loadedBuild : null;
  if (saved) return saved;
  let build = null;
  try {
    const res = await fetch(chrome.runtime.getURL('build.json'));
    build = (await res.json()).build || null;
  } catch {
    build = null;
  }
  if (build && session) await session.set({ loadedBuild: build });
  return build;
}

ownBuild();

async function reloadIfRebuilt(appBuild) {
  const mine = await ownBuild();
  if (!mine || mine === appBuild) return;
  const { reloadedForBuild } = await chrome.storage.local.get('reloadedForBuild');
  if (reloadedForBuild === appBuild) return;
  await chrome.storage.local.set({ reloadedForBuild: appBuild });
  chrome.runtime.reload();
}

async function reloadIfUpdated(appVersion) {
  if (!isNewer(appVersion, chrome.runtime.getManifest().version)) return;
  const { reloadedFor } = await chrome.storage.local.get('reloadedFor');
  if (reloadedFor === appVersion) return;
  await chrome.storage.local.set({ reloadedFor: appVersion });
  chrome.runtime.reload();
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function ensureRunning() {
  const info = await ping(true);
  if (info) return info;
  if (!chrome.runtime.sendNativeMessage) return null;
  if (!launching) {
    launching = (async () => {
      try {
        const r = await chrome.runtime.sendNativeMessage(NATIVE_HOST, { action: 'launch' });
        if (!r?.ok) return null;
      } catch {
        return null;
      }
      for (let i = 0; i < 40; i++) {
        await sleep(500);
        const v = await ping(true);
        if (v) return v;
      }
      return null;
    })().finally(() => setTimeout(() => (launching = null), 0));
  }
  return launching;
}

function removeSelf() {
  const management = chrome.management;
  if (!management?.uninstallSelf) return;
  Promise.resolve(management.uninstallSelf({ showConfirmDialog: false })).catch(() => {});
}

const titleCache = new Map();

function oembedUrl(url) {
  let u;
  try {
    u = new URL(url);
  } catch {
    return null;
  }
  const h = u.hostname.replace(/^www\./, '').replace(/^m\./, '');
  const enc = (v) => encodeURIComponent(v);
  if (h === 'youtu.be' || h === 'youtube.com' || h.endsWith('.youtube.com')) {
    const id = h === 'youtu.be' ? u.pathname.slice(1).split('/')[0] : u.searchParams.get('v') || u.pathname.match(/^\/(shorts|live)\/([\w-]{6,})/)?.[2];
    return id ? 'https://www.youtube.com/oembed?format=json&url=' + enc('https://www.youtube.com/watch?v=' + id) : null;
  }
  if (h === 'vimeo.com' || h.endsWith('.vimeo.com')) return 'https://vimeo.com/api/oembed.json?url=' + enc(url);
  if (h === 'tiktok.com' || h.endsWith('.tiktok.com')) return 'https://www.tiktok.com/oembed?url=' + enc(url);
  if (h === 'soundcloud.com') return 'https://soundcloud.com/oembed?format=json&url=' + enc(url);
  if (h === 'dailymotion.com') return 'https://www.dailymotion.com/services/oembed?format=json&url=' + enc(url);
  if (h === 'reddit.com' || h.endsWith('.reddit.com')) return 'https://www.reddit.com/oembed?url=' + enc(url);
  if (h === 'x.com' || h === 'twitter.com') return 'https://publish.twitter.com/oembed?omit_script=1&url=' + enc(url);
  return null;
}

const sizeCache = new Map();

function qualitySizes(url) {
  if (!url) return Promise.resolve(null);
  if (!sizeCache.has(url)) {
    const pending = call('sizes', { url }, 100000)
      .then((r) => (r.ok ? r.data?.sizes ?? null : null))
      .catch(() => null)
      .then((sizes) => {
        if (!sizes) sizeCache.delete(url);
        return sizes;
      });
    sizeCache.set(url, pending);
    if (sizeCache.size > 50) sizeCache.delete(sizeCache.keys().next().value);
  }
  return sizeCache.get(url);
}

async function lookupTitle(url) {
  if (titleCache.has(url)) return titleCache.get(url);
  const endpoint = oembedUrl(url);
  let title = null;
  if (endpoint) {
    try {
      const ctrl = new AbortController();
      const timer = setTimeout(() => ctrl.abort(), 6000);
      const res = await fetch(endpoint, { signal: ctrl.signal, credentials: 'omit' });
      clearTimeout(timer);
      if (res.ok) {
        const data = await res.json();
        title = data.title || null;
        if (!title && data.html) {
          const textOnly = String(data.html).replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ').trim();
          title = textOnly.split(' — ')[0].slice(0, 120) || null;
        }
        if (!title && data.author_name) title = data.author_name;
      }
    } catch {
      title = null;
    }
  }
  if (titleCache.size > 500) titleCache.clear();
  titleCache.set(url, title);
  return title;
}

async function autoPair() {
  const { autoPairPending } = await chrome.storage.local.get({ autoPairPending: false });
  if (!autoPairPending) return;
  await chrome.storage.local.set({ autoPairPending: false });
  try {
    await call('pair', {}, 4000);
  } catch {
    await chrome.storage.local.set({ autoPairPending: true });
  }
}

function updateBadge(info) {
  const text = !info ? '!' : info.paired ? '' : '?';
  chrome.action.setBadgeText({ text }).catch(() => {});
  chrome.action.setBadgeBackgroundColor({ color: !info ? '#ff5a52' : '#ffb340' }).catch(() => {});
}

function notify(message, title = 'Multron Download Manager') {
  chrome.notifications
    .create({ type: 'basic', iconUrl: 'icons/icon128.png', title, message })
    .catch(() => {});
}

async function cookiesFor(urls) {
  const { sendCookies } = await settings();
  if (!sendCookies) return [];
  const seen = new Map();
  for (const url of urls.filter(Boolean)) {
    try {
      const list = await chrome.cookies.getAll({ url });
      for (const c of list) {
        seen.set(`${c.domain}|${c.path}|${c.name}`, {
          name: c.name,
          value: c.value,
          domain: c.domain,
          path: c.path,
          secure: c.secure,
          httpOnly: c.httpOnly,
          expirationDate: c.expirationDate
        });
      }
    } catch {
    }
  }
  return [...seen.values()];
}

async function send(req) {
  const cookieUrls = [req.url, req.referrer];
  if (req.kind === 'media' && hostOf(req.url).includes('instagram.com')) cookieUrls.push('https://www.instagram.com/');
  if (req.kind === 'media' && (hostOf(req.url).includes('youtube.com') || hostOf(req.url) === 'youtu.be')) cookieUrls.push('https://www.youtube.com/');
  const body = {
    userAgent: navigator.userAgent,
    ...req,
    cookies: req.url.startsWith('magnet:') ? [] : await cookiesFor(cookieUrls)
  };
  let r;
  try {
    await ensureRunning();
    r = await call('download', body, 15000);
  } catch {
    notify('Multron Download Manager is not running and could not be started. Start the app and try again.');
    return false;
  }
  if (r.status === 401) {
    notify('Approve this browser in the Multron Download Manager window, then try again.', 'Multron Download Manager: connection request');
    pingCache.at = 0;
    return false;
  }
  if (!r.ok) {
    notify(r.data?.error ?? `The app rejected the download (HTTP ${r.status}).`);
    return false;
  }
  return true;
}

async function grabPage(tabId, pages = 0) {
  let page;
  try {
    page = await chrome.tabs.sendMessage(tabId, { type: 'grab', pages });
  } catch {
    return { error: 'Reload the page and try again (the extension is not active on it yet).' };
  }
  if (!page?.items?.length) {
    notify('Nothing to download was found on this page.');
    return { error: 'Nothing to download was found on this page.' };
  }
  const hosts = [...new Set([page.pageUrl, ...page.items.map((i) => i.url)].map((u) => {
    try {
      return new URL(u).origin + '/';
    } catch {
      return null;
    }
  }).filter(Boolean))].slice(0, 12);
  const body = { ...page, userAgent: navigator.userAgent, cookies: await cookiesFor(hosts) };
  let r;
  try {
    await ensureRunning();
    r = await call('batch', body, 15000);
  } catch {
    notify('Multron Download Manager is not running. Start the app and try again.');
    return { error: 'Multron Download Manager is not running.' };
  }
  if (r.status === 401) {
    notify('Approve this browser in the Multron Download Manager window, then try again.', 'Multron Download Manager: connection request');
    return { error: 'Approve this browser in Multron Download Manager first.' };
  }
  if (!r.ok) return { error: r.data?.error ?? `HTTP ${r.status}` };
  return { ok: true, count: page.items.length };
}

function createMenus() {
  chrome.contextMenus.removeAll(() => {
    const add = (id, title, contexts) => chrome.contextMenus.create({ id, title, contexts });
    add('multron-link', 'Download link with Multron Download Manager', ['link']);
    add('multron-link-media', 'Download video/audio from link with Multron Download Manager', ['link']);
    add('multron-media', 'Download this media with Multron Download Manager', ['video', 'audio', 'image']);
    add('multron-page', 'Download video on this page with Multron Download Manager', ['page']);
    add('multron-grab', 'Download all from this page with Multron Download Manager…', ['page']);
    add('multron-selection', 'Download selected link with Multron Download Manager', ['selection']);
  });
}

chrome.runtime.onInstalled.addListener(async (details) => {
  createMenus();
  if (details.reason === 'install') {
    await chrome.storage.local.set({ autoPairPending: true });
    const access = await chrome.permissions.contains({ origins: ['<all_urls>'] }).catch(() => true);
    if (!access) chrome.tabs.create({ url: chrome.runtime.getURL('popup.html') });
  }
  ping(true);
});
chrome.runtime.onStartup?.addListener(() => {
  createMenus();
  ping(true);
});

chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  const page = info.pageUrl || tab?.url || '';
  const title = tab?.title || null;
  switch (info.menuItemId) {
    case 'multron-link':
      await send({ url: info.linkUrl, kind: 'auto', referrer: page });
      break;
    case 'multron-link-media':
      await send({ url: info.linkUrl, kind: 'media', referrer: page });
      break;
    case 'multron-media': {
      const src = info.srcUrl || '';
      if (!src || src.startsWith('blob:') || src.startsWith('data:')) {
        await send({ url: page, kind: 'media', pageTitle: title, referrer: page });
      } else {
        await send({ url: src, kind: isMediaUrl(page) ? 'media' : 'auto', referrer: page, pageTitle: title });
      }
      break;
    }
    case 'multron-page':
      await send({ url: page, kind: 'media', pageTitle: title, referrer: page });
      break;
    case 'multron-grab':
      if (tab?.id !== undefined) await grabPage(tab.id, 0);
      break;
    case 'multron-selection': {
      const text = (info.selectionText || '').trim();
      if (/^(https?:\/\/|magnet:)/i.test(text)) await send({ url: text, kind: 'auto', referrer: page });
      else notify('The selection is not a link.');
      break;
    }
  }
});

const decidesFilename = !!chrome.downloads.onDeterminingFilename;

if (decidesFilename) {
  chrome.downloads.onDeterminingFilename.addListener((item, suggest) => {
    const url = item.finalUrl || item.url || '';
    if (ownDownloads.has(url) || !/^https?:/i.test(url)) {
      ownDownloads.delete(url);
      suggest();
      return;
    }
    takeOver(item, url, () => suggest()).catch(() => suggest());
    return true;
  });
} else {
  chrome.downloads.onCreated.addListener((item) => {
    const url = item.finalUrl || item.url || '';
    if (ownDownloads.has(url)) {
      ownDownloads.delete(url);
      return;
    }
    if (!/^https?:/i.test(url)) return;
    chrome.downloads.pause(item.id).catch(() => {});
    const resume = () => chrome.downloads.resume(item.id).catch(() => {});
    takeOver(item, url, resume).catch(resume);
  });
}

async function waitForApproval(ms) {
  const until = Date.now() + ms;
  while (Date.now() < until) {
    await new Promise((r) => setTimeout(r, 2000));
    const info = await ping(true);
    if (info?.paired) return true;
  }
  return false;
}

async function takeOver(item, url, giveBack) {
  const s = await settings();
  if (!s.intercept) return giveBack();
  const host = hostOf(url);
  const skip = s.skipDomains
    .split(/[\s,]+/)
    .map((d) => d.trim().toLowerCase())
    .filter(Boolean);
  if (skip.some((d) => host === d || host.endsWith('.' + d))) return giveBack();
  const size = item.totalBytes > 0 ? item.totalBytes : item.fileSize;
  if (s.minSizeKB > 0 && size > 0 && size < s.minSizeKB * 1024) return giveBack();

  const info = await ensureRunning();
  if (!info) return giveBack();
  if (!info.paired) {
    call('pair', {}, 4000).catch(() => {});
    notify('Approve this browser in the Multron Download Manager window. The download waits until then.', 'Multron Download Manager: connection request');
    if (!(await waitForApproval(60000))) return giveBack();
  }

  try {
    await chrome.downloads.cancel(item.id);
    await chrome.downloads.erase({ id: item.id });
  } catch {
  }
  const filename = item.filename ? item.filename.split(/[\\/]/).pop() : null;
  const ok = await send({
    url,
    kind: url.toLowerCase().split('?')[0].endsWith('.torrent') ? 'torrent' : 'auto',
    filename,
    referrer: item.referrer || null,
    total: item.totalBytes > 0 ? item.totalBytes : null
  });
  if (!ok) {
    ownDownloads.add(url);
    chrome.downloads.download({ url }).catch(() => ownDownloads.delete(url));
  }
}

chrome.runtime.onMessage.addListener((msg, sender, reply) => {
  (async () => {
    switch (msg?.type) {
      case 'ping':
        reply(await ping(!!msg.force));
        break;
      case 'pair': {
        try {
          const r = await call('pair', {}, 4000);
          pingCache.at = 0;
          reply(r.data ?? { paired: false });
        } catch {
          reply(null);
        }
        break;
      }
      case 'grab-page':
        reply(await grabPage(msg.tabId, msg.pages));
        break;
      case 'link-download': {
        const payload = { referrer: sender?.tab?.url ?? null, ...msg.payload };
        if (!(await ensureRunning())) {
          ownDownloads.add(payload.url);
          chrome.downloads.download({ url: payload.url }).catch(() => ownDownloads.delete(payload.url));
          reply('browser');
        } else {
          reply(await send(payload));
        }
        break;
      }
      case 'download':
        reply(await send({ referrer: sender?.tab?.url ?? null, ...msg.payload }));
        break;
      case 'settings':
        ping().catch(() => {});
        reply(await settings());
        break;
      case 'title':
        reply(await lookupTitle(String(msg.url || '')));
        break;
      case 'sizes':
        reply(await qualitySizes(String(msg.url || '')));
        break;
      default:
        reply(null);
    }
  })();
  return true;
});

chrome.alarms?.create('multron-ping', { periodInMinutes: 0.5 });
chrome.alarms?.onAlarm.addListener((alarm) => {
  if (alarm.name === 'multron-ping') ping(true);
});

createMenus();
ping(true);
