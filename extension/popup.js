const $ = (id) => document.getElementById(id);
const MEDIA = /(^|\.)(youtube\.com|youtu\.be|reddit\.com|redd\.it|instagram\.com|tiktok\.com|twitter\.com|x\.com|facebook\.com|fb\.watch|vimeo\.com|dailymotion\.com|twitch\.tv|soundcloud\.com|bilibili\.com|streamable\.com|threads\.net|kick\.com)$/;
const FILES = /(^|\.)(drive\.google\.com|docs\.google\.com|mega\.nz|mega\.io|mediafire\.com|dropbox\.com|pixeldrain\.com)$/;

let tab = null;

function msg(text) {
  $('msg').textContent = text;
}

async function refreshStatus(force = false) {
  const info = await chrome.runtime.sendMessage({ type: 'ping', force });
  const dot = document.querySelector('.dot');
  dot.className = 'dot';
  if (!info) {
    dot.classList.add('bad');
    $('status-text').textContent = 'App not running';
    $('connect').hidden = true;
  } else if (!info.paired) {
    dot.classList.add('warn');
    $('status-text').textContent = `Not approved · v${info.version}`;
    $('connect').hidden = false;
  } else {
    dot.classList.add('ok');
    $('status-text').textContent = `Connected · v${info.version}`;
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
  status.textContent = 'Checking sizes…';
  const sizes = await chrome.runtime.sendMessage({ type: 'sizes', url }).catch(() => null);
  options.forEach((o, i) => {
    const size = sizeText(sizes?.[o.value]);
    o.textContent = size ? `${names[i]} · ${size}` : names[i];
  });
  status.classList.remove('busy');
  status.textContent = sizes ? '' : 'Sizes are not available for this page';
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
}

async function sendDownload(payload) {
  msg('Sending…');
  const ok = await chrome.runtime.sendMessage({ type: 'download', payload });
  msg(ok ? 'Sent to Multron Download Manager ✓' : 'Multron Download Manager did not accept it — check the app.');
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
    $('page-label').textContent = tab.title || 'This page';
    showSizes(tab.url);
  } else if (FILES.test(host)) {
    $('page-box').hidden = false;
    $('quality').hidden = true;
    $('page-label').textContent = 'File on this page';
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
    msg(pages ? 'Scrolling and collecting…' : 'Collecting…');
    const r = await chrome.runtime.sendMessage({ type: 'grab-page', tabId: tab.id, pages });
    $('grab').disabled = false;
    msg(r?.ok ? `Found ${r.count} items. Pick them in Multron.` : r?.error ?? 'Something went wrong.');
  });

  $('send-link').addEventListener('click', () => {
    const url = $('link').value.trim();
    if (!/^(https?:\/\/|magnet:)/i.test(url)) return msg('Enter an http(s) or magnet link.');
    sendDownload({ url, kind: 'auto', referrer: tab?.url ?? null });
  });
  $('link').addEventListener('keydown', (e) => e.key === 'Enter' && $('send-link').click());

  const ALL_SITES = { origins: ['<all_urls>'] };
  const hasAccess = await chrome.permissions.contains(ALL_SITES).catch(() => true);
  $('perm-box').hidden = hasAccess;
  $('grant').addEventListener('click', async () => {
    const ok = await chrome.permissions.request(ALL_SITES).catch(() => false);
    $('perm-box').hidden = ok;
    msg(ok ? 'Access granted. Reload the page to see the button.' : 'Access was not granted.');
  });

  $('connect').addEventListener('click', async () => {
    const r = await chrome.runtime.sendMessage({ type: 'pair' });
    msg(r?.paired ? 'Connected ✓' : 'Approve the request in the Multron Download Manager window.');
    setTimeout(() => refreshStatus(true), 1500);
  });
});
