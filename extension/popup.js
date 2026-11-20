const $ = (id) => document.getElementById(id);
const MEDIA = /(^|\.)(youtube\.com|youtu\.be|reddit\.com|redd\.it|instagram\.com|tiktok\.com|twitter\.com|x\.com|facebook\.com|fb\.watch|vimeo\.com|dailymotion\.com|twitch\.tv|soundcloud\.com|bilibili\.com|streamable\.com|threads\.net|kick\.com)$/;
const FILES = /(^|\.)(drive\.google\.com|docs\.google\.com|mega\.nz|mega\.io|mediafire\.com|dropbox\.com|pixeldrain\.com)$/;

let tab = null;

let uiMessages = null;

function fill(text, subs) {
  return subs.reduce((s, v, i) => s.split('$' + (i + 1)).join(String(v)), text);
}

function tr(text, ...subs) {
  const id = text.replace(/[^A-Za-z0-9]+/g, '_').replace(/^_+|_+$/g, '').slice(0, 120);
  if (uiMessages) return fill(uiMessages[id] || text, subs);
  let out = '';
  try {
    out = chrome.i18n.getMessage(id, subs.map(String));
  } catch {
    out = '';
  }
  return out || fill(text, subs);
}

function localize() {
  for (const el of document.querySelectorAll('[data-i18n]')) {
    const sub = el.dataset.i18nSub;
    if (!el.dataset.i18n) el.dataset.i18n = el.textContent.trim();
    el.textContent = tr(el.dataset.i18n, ...(sub ? [sub] : []));
  }
  for (const el of document.querySelectorAll('[data-i18n-title]')) {
    if (!el.dataset.i18nTitle) el.dataset.i18nTitle = el.title;
    el.title = tr(el.dataset.i18nTitle);
  }
  for (const el of document.querySelectorAll('[data-i18n-placeholder]')) {
    if (!el.dataset.i18nPlaceholder) el.dataset.i18nPlaceholder = el.placeholder;
    el.placeholder = tr(el.dataset.i18nPlaceholder);
  }
}

localize();

const THEME_ATTR = 'data-theme';
function applyTheme(setting) {
  const dark = setting === 'dark' || (setting !== 'light' && !window.matchMedia?.('(prefers-color-scheme: light)').matches);
  document.documentElement.setAttribute(THEME_ATTR, dark ? 'dark' : 'light');
}
chrome.storage.local.get({ uiTheme: 'system' }).then(({ uiTheme }) => applyTheme(uiTheme)).catch(() => applyTheme('system'));
chrome.storage.onChanged.addListener((changes, area) => {
  if (area === 'local' && changes.uiTheme) applyTheme(changes.uiTheme.newValue);
});

chrome.storage.local.get({ uiMessages: null }).then(({ uiMessages: m }) => {
  if (m && typeof m === 'object') {
    uiMessages = m;
    localize();
    refreshStatus();
  }
}).catch(() => {});

chrome.storage.onChanged.addListener((changes, area) => {
  if (area === 'local' && changes.uiMessages?.newValue) {
    uiMessages = changes.uiMessages.newValue;
    localize();
    refreshStatus();
  }
});

function msg(text) {
  $('msg').textContent = text;
}

async function refreshStatus(force = false) {
  const info = await chrome.runtime.sendMessage({ type: 'ping', force });
  const dot = document.querySelector('.dot');
  dot.className = 'dot';
  if (!info) {
    dot.classList.add('bad');
    $('status-text').textContent = tr('App not running');
    $('connect').hidden = true;
  } else if (!info.paired) {
    dot.classList.add('warn');
    $('status-text').textContent = tr('Not approved · v$1', info.version);
    $('connect').hidden = false;
  } else {
    dot.classList.add('ok');
    $('status-text').textContent = tr('Connected · v$1', info.version);
    $('connect').hidden = true;
  }
  return info;
}

function sizeText(s) {
  if (!s?.size) return '';
  const gb = s.size / 1073741824;
  const text = gb >= 1 ? `${gb.toFixed(gb >= 10 ? 0 : 1)} GB` : `${Math.max(0.1, s.size / 1048576).toFixed(1)} MB`;
  return (s.exact ? '' : '~') + text;
}

async function showSizes(url) {
  const options = [...$('quality').options];
  const names = options.map((o) => o.textContent);
  const status = $('size-status');
  status.hidden = false;
  status.classList.add('busy');
  status.textContent = tr('Checking sizes…');
  const sizes = await chrome.runtime.sendMessage({ type: 'sizes', url }).catch(() => null);
  options.forEach((o, i) => {
    const size = sizeText(sizes?.[o.value]);
    o.textContent = size ? `${names[i]} · ${size}` : names[i];
  });
  status.classList.remove('busy');
  status.textContent = sizes ? '' : tr('Sizes are not available for this page');
  status.hidden = !!sizes;
}

async function loadOptions() {
  const s = await chrome.runtime.sendMessage({ type: 'settings' });
  for (const key of ['intercept', 'interceptMagnets', 'mediaButton', 'sendCookies']) {
    $(key).checked = !!s[key];
    $(key).addEventListener('change', () => chrome.storage.local.set({ [key]: $(key).checked }));
  }
  for (const key of ['minSizeKB', 'port']) {
    $(key).value = s[key];
    $(key).addEventListener('change', async () => {
      await chrome.storage.local.set({ [key]: Number($(key).value) || 0 });
      if (key === 'port') refreshStatus(true);
    });
  }
  $('skipDomains').value = s.skipDomains;
  $('skipDomains').addEventListener('change', () => chrome.storage.local.set({ skipDomains: $('skipDomains').value }));
  $('skipExtensions').value = s.skipExtensions || '';
  $('skipExtensions').addEventListener('change', () => chrome.storage.local.set({ skipExtensions: $('skipExtensions').value }));
}

async function sendDownload(payload) {
  msg(tr('Sending…'));
  const ok = await chrome.runtime.sendMessage({ type: 'download', payload });
  msg(ok ? `${tr('Sent to Multron Download Manager')} ✓` : tr('Multron Download Manager did not accept it — check the app.'));
}

document.addEventListener('DOMContentLoaded', async () => {
  [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  await loadOptions();
  await refreshStatus(true);

  let host = '';
  try {
    host = new URL(tab?.url || '').hostname.replace(/^www\./, '');
  } catch {
    host = '';
  }
  if (MEDIA.test(host)) {
    $('page-box').hidden = false;
    $('page-label').textContent = tab.title || tr('This page');
    showSizes(tab.url);
  } else if (FILES.test(host)) {
    $('page-box').hidden = false;
    $('quality').hidden = true;
    $('page-label').textContent = tr('File on this page');
  }

  $('container').closest('.format-row').hidden = !MEDIA.test(host);
  chrome.storage.local.get('videoFormat').then((v) => ($('container').value = v?.videoFormat || 'default')).catch(() => {});
  $('container').addEventListener('change', () => chrome.storage.local.set({ videoFormat: $('container').value }));

  $('send-page').addEventListener('click', () => {
    const media = MEDIA.test(host);
    const quality = media ? $('quality').value : null;
    const container = $('container').value;
    sendDownload({
      url: tab.url,
      kind: media ? 'media' : 'http',
      quality,
      container: media && !String(quality).startsWith('audio') && container !== 'default' ? container : undefined,
      pageTitle: tab.title,
      referrer: tab.url
    });
  });

  $('grab').addEventListener('click', async () => {
    if (!tab?.id) return;
    const pages = Number($('grab-pages').value) || 0;
    $('grab').disabled = true;
    msg(pages ? tr('Scrolling and collecting…') : tr('Collecting…'));
    const r = await chrome.runtime.sendMessage({ type: 'grab-page', tabId: tab.id, pages });
    $('grab').disabled = false;
    msg(r?.ok ? tr('Found $1 items. Pick them in Multron.', r.count) : r?.error ?? tr('Something went wrong.'));
  });

  $('send-link').addEventListener('click', () => {
    const url = $('link').value.trim();
    if (!/^(https?:\/\/|magnet:)/i.test(url)) return msg(tr('Enter an http(s) or magnet link.'));
    sendDownload({ url, kind: 'auto', referrer: tab?.url ?? null });
  });
  $('link').addEventListener('keydown', (e) => e.key === 'Enter' && $('send-link').click());

  const ALL_SITES = { origins: ['<all_urls>'] };
  const hasAccess = await chrome.permissions.contains(ALL_SITES).catch(() => true);
  $('perm-box').hidden = hasAccess;
  $('grant').addEventListener('click', async () => {
    const ok = await chrome.permissions.request(ALL_SITES).catch(() => false);
    $('perm-box').hidden = ok;
    msg(ok ? tr('Access granted. Reload the page to see the button.') : tr('Access was not granted.'));
  });

  $('connect').addEventListener('click', async () => {
    const r = await chrome.runtime.sendMessage({ type: 'pair' });
    msg(r?.paired ? tr('Connected ✓') : tr('Approve the request in the Multron Download Manager window.'));
    setTimeout(() => refreshStatus(true), 1500);
  });
});
