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
  } else if (FILES.test(host)) {
    $('page-box').hidden = false;
    $('quality').hidden = true;
    $('page-label').textContent = 'File on this page';
  }

  $('send-page').addEventListener('click', () => {
    const media = MEDIA.test(host);
    sendDownload({
      url: tab.url,
      kind: media ? 'media' : 'http',
      quality: media ? $('quality').value : null,
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
