(() => {
  if (window.top !== window || window.__multronLoaded) return;
  window.__multronLoaded = true;

  const host = location.hostname.replace(/^www\./, '').replace(/^m\./, '');
  const on = (...domains) => domains.some((d) => host === d || host.endsWith('.' + d));

  function mediaPage() {
    const p = location.pathname;
    if (on('youtube.com')) return p === '/watch' || p.startsWith('/shorts/') || p.startsWith('/live/');
    if (on('youtu.be')) return p.length > 1;
    if (on('reddit.com')) return /\/comments\//.test(p);
    if (on('instagram.com')) return /^\/(p|reel|reels|tv)\//.test(p) || /^\/[^/]+\/(p|reel)\//.test(p);
    if (on('x.com', 'twitter.com')) return /\/status\/\d+/.test(p);
    if (on('tiktok.com')) return /\/video\/\d+/.test(p) || p.startsWith('/t/');
    if (on('vimeo.com')) return /^\/\d+/.test(p);
    if (on('facebook.com')) return /\/(videos|watch|reel)\b/.test(p) || location.search.includes('v=');
    if (on('twitch.tv')) return /\/(videos|clip)\//.test(p) || on('clips.twitch.tv');
    if (on('dailymotion.com')) return p.startsWith('/video/');
    if (on('soundcloud.com')) return p.split('/').filter(Boolean).length >= 2;
    if (on('bilibili.com')) return p.startsWith('/video/');
    if (on('streamable.com', 'kick.com', 'threads.net', 'bandcamp.com')) return p.length > 1;
    return false;
  }

  function filePage() {
    const p = location.pathname;
    if (on('drive.google.com')) return /\/file\/d\//.test(p) || location.search.includes('id=');
    if (on('docs.google.com')) return /\/(document|spreadsheets|presentation)\/d\//.test(p);
    if (on('mega.nz', 'mega.io')) return p.startsWith('/file/') || location.hash.startsWith('#!');
    if (on('mediafire.com')) return p.startsWith('/file/');
    if (on('dropbox.com')) return /^\/(s|scl)\//.test(p);
    if (on('pixeldrain.com')) return p.startsWith('/u/');
    return false;
  }

  const VIDEO_EXT = /\.(mp4|mkv|avi|mov|wmv|webm|m4v|flv|ogv|ts|m3u8)$/i;
  const AUDIO_EXT = /\.(mp3|flac|wav|m4a|aac|ogg|opus|wma)$/i;
  const IMAGE_EXT = /\.(jpe?g|png|gif|webp|avif|bmp|tiff?|heic)$/i;
  const FILE_EXT =
    /\.(zip|rar|7z|tar|gz|tgz|bz2|xz|zst|iso|img|exe|msi|msix|appx|dmg|pkg|deb|rpm|apk|appimage|flatpak|bin|jar|torrent|mp4|mkv|avi|mov|wmv|webm|m4v|flv|mp3|flac|wav|m4a|aac|ogg|opus|wma|cab|vhd|vhdx|ova|epub|mobi|pdf)$/i;

  function absolute(href) {
    try {
      const u = new URL(href, location.href);
      return u.protocol === 'http:' || u.protocol === 'https:' ? u : null;
    } catch {
      return null;
    }
  }

  const pathOf = (u) => {
    try {
      return decodeURIComponent(u.pathname);
    } catch {
      return u.pathname;
    }
  };

  function isFileLink(a) {
    const u = absolute(a.href);
    if (!u) return false;
    if (a.hasAttribute('download')) return true;
    return FILE_EXT.test(pathOf(u)) && !/\.pdf$/i.test(pathOf(u));
  }

  const QUALITIES = [
    ['best', 'Best quality'],
    ['1080', '1080p'],
    ['720', '720p'],
    ['480', '480p'],
    ['audio-mp3', 'Audio (MP3)'],
    ['audio-m4a', 'Audio (M4A)']
  ];

  function meta(name) {
    const el = document.querySelector(`meta[property="${name}"], meta[name="${name}"]`);
    return el?.getAttribute('content') || null;
  }

  const text = (el) => (el?.textContent || '').replace(/\s+/g, ' ').trim();

  function toast(message, ok = true) {
    const el = document.createElement('div');
    el.className = 'multron-toast' + (ok ? '' : ' multron-toast-bad');
    el.textContent = message;
    document.documentElement.appendChild(el);
    setTimeout(() => el.remove(), 3500);
  }

  async function send(payload) {
    const ok = await chrome.runtime.sendMessage({ type: 'download', payload });
    toast(ok ? 'Sent to Multron Download Manager' : 'Multron Download Manager did not accept the download', !!ok);
  }

  const REDDIT_PLAYER = 'shreddit-player, shreddit-player-2';

  function redditPlayerItem(player) {
    if (!player || player.hasAttribute('post-promoted')) return null;
    const src = player.getAttribute('src');
    if (!src || !/^https?:/i.test(src)) return null;
    const comment = player.closest('shreddit-comment');
    const title =
      player.getAttribute('post-title') ||
      player.closest('shreddit-post')?.getAttribute('post-title') ||
      (comment ? redditCommentName(comment) : null) ||
      document.title;
    return {
      url: src,
      kind: 'media',
      mediaType: 'video',
      name: title,
      filename: title,
      thumbnail: player.getAttribute('poster') || null
    };
  }

  const MEDIA_LINK = /^(?:[\w-]+\.)?(v\.redd\.it|youtube\.com|youtu\.be|streamable\.com|vimeo\.com|gfycat\.com|redgifs\.com|imgur\.com|i\.imgur\.com|x\.com|twitter\.com|tiktok\.com|instagram\.com|twitch\.tv|clips\.twitch\.tv|kick\.com|dailymotion\.com|giphy\.com|media\.giphy\.com)$/i;

  function redditVideo(href) {
    const m = href.match(/^https?:\/\/v\.redd\.it\/([a-z0-9]+)\/?(?:[?#].*)?$/i);
    return m ? `https://v.redd.it/${m[1]}/HLSPlaylist.m3u8` : href;
  }

  function redditCommentName(comment) {
    const author = comment.getAttribute('author');
    const body = text(comment.querySelector('[slot="comment"], .md')).slice(0, 60);
    const who = author ? `u/${author}` : 'comment';
    return body ? `${who}: ${body}` : `Reddit ${who}`;
  }

  function redditCommentItems(add) {
    const post = document.querySelector('shreddit-post')?.getAttribute('post-title') || meta('og:title') || document.title;
    const comments = document.querySelectorAll('shreddit-comment, .thing.comment');
    for (const comment of comments) {
      const own = (el) => el.closest('shreddit-comment, .thing.comment') === comment;
      const isOld = comment.classList.contains('comment');
      const name = isOld
        ? (() => {
            const author = comment.getAttribute('data-author');
            const body = text(comment.querySelector('.usertext-body .md')).slice(0, 60);
            return (author ? `u/${author}` : 'comment') + (body ? `: ${body}` : '');
          })()
        : redditCommentName(comment);
      const label = `${post} — ${name}`;
      for (const player of comment.querySelectorAll(REDDIT_PLAYER)) {
        if (!own(player)) continue;
        const item = redditPlayerItem(player);
        if (item) add({ ...item, name: label, filename: label });
      }
      for (const v of comment.querySelectorAll('video')) {
        if (!own(v)) continue;
        const src = [v.currentSrc, v.src, ...[...v.querySelectorAll('source[src]')].map((x) => x.src)].find((x) => x && /^https?:/i.test(x));
        if (src && !/\.m3u8|\.mpd/i.test(src)) add({ url: src, kind: 'auto', mediaType: 'video', name: label, thumbnail: v.poster || null });
      }
      for (const img of comment.querySelectorAll('img')) {
        if (!own(img)) continue;
        const src = bestImage(img);
        if (!src) continue;
        const u = absolute(src);
        if (!u || !/(^|\.)(preview\.redd\.it|i\.redd\.it|external-preview\.redd\.it|giphy\.com)$/i.test(u.hostname)) continue;
        if (/emoji|avatar|snoo|award/i.test(u.pathname + (img.className || ''))) continue;
        const link = img.closest('a[href]');
        const full = link && /preview\.redd\.it|i\.redd\.it/.test(link.href) ? link.href : u.hostname === 'i.redd.it' ? fullSize(src) : src;
        const gif = /\.gif|format=mp4/i.test(full);
        add({ url: full, kind: 'auto', mediaType: gif ? 'video' : 'image', name: label, thumbnail: src });
      }
      for (const a of comment.querySelectorAll('a[href]')) {
        if (!own(a)) continue;
        const u = absolute(a.getAttribute('href'));
        if (!u) continue;
        const host = u.hostname.replace(/^www\./, '').replace(/^m\./, '').replace(/^old\./, '');
        const p = pathOf(u);
        if (/^(i\.redd\.it|preview\.redd\.it)$/i.test(host)) {
          add({ url: host === 'i.redd.it' ? fullSize(u.href) : u.href, kind: 'auto', mediaType: /\.(gif|mp4)$/i.test(p) ? 'video' : 'image', name: label });
        } else if (MEDIA_LINK.test(host)) {
          if (/^(x|twitter)\.com$/i.test(host) && !/\/status\/\d+/.test(p)) continue;
          if (/^instagram\.com$/i.test(host) && !/\/(p|reel|tv)\//.test(p)) continue;
          if (/^(youtube\.com)$/i.test(host) && !u.searchParams.get('v') && !/^\/(shorts|live)\//.test(p)) continue;
          add({ url: redditVideo(u.href), kind: 'media', mediaType: 'video', name: label, filename: /v\.redd\.it/i.test(host) ? label : undefined });
        } else if (VIDEO_EXT.test(p)) {
          add({ url: u.href, kind: 'auto', mediaType: 'video', name: label });
        }
      }
    }
  }

  function postUrlFor(el) {
    const pick = (root, selector) => {
      const a = root?.querySelector(selector);
      return a ? absolute(a.getAttribute('href'))?.href ?? null : null;
    };
    if (on('instagram.com')) {
      const post = el.closest('article') || el.closest('[role="presentation"]');
      return pick(post, 'a[href*="/p/"], a[href*="/reel/"], a[href*="/tv/"]') || (mediaPage() ? location.href : null);
    }
    if (on('x.com', 'twitter.com')) {
      return pick(el.closest('article'), 'a[href*="/status/"]:has(time)') || pick(el.closest('article'), 'a[href*="/status/"]') || (mediaPage() ? location.href : null);
    }
    if (on('youtube.com')) {
      const card = el.closest('ytd-rich-item-renderer, ytd-video-renderer, ytd-compact-video-renderer, ytd-reel-item-renderer, ytd-grid-video-renderer');
      return pick(card, 'a[href*="/watch?v="], a[href*="/shorts/"]') || (mediaPage() ? location.href : null);
    }
    if (on('tiktok.com')) {
      const card = el.closest('[data-e2e="recommend-list-item-container"], article, [class*="DivItemContainer"]');
      return pick(card, 'a[href*="/video/"]') || (mediaPage() ? location.href : null);
    }
    if (on('facebook.com')) {
      const card = el.closest('[role="article"], [data-pagelet]');
      return pick(card, 'a[href*="/videos/"], a[href*="/reel/"], a[href*="/watch/?v="]') || (mediaPage() ? location.href : null);
    }
    return null;
  }

  const YT_CARDS = 'ytd-rich-item-renderer, ytd-video-renderer, ytd-compact-video-renderer, ytd-reel-item-renderer, ytd-grid-video-renderer, ytd-playlist-video-renderer, yt-lockup-view-model';

  function youtubeId(href) {
    try {
      const u = new URL(href, location.href);
      if (u.hostname.endsWith('youtu.be')) return u.pathname.slice(1).split('/')[0] || null;
      return u.searchParams.get('v') || u.pathname.match(/^\/(shorts|live)\/([\w-]{6,})/)?.[2] || null;
    } catch {
      return null;
    }
  }

  const youtubeTitle = (t) => (t || '').replace(/^\(\d+\+?\)\s*/, '').replace(/\s+-\s+YouTube$/, '').trim();


  const SITE_WORDS = /^(youtube|youtube music|youtube shorts|instagram|tiktok|x|twitter|facebook|reddit|vimeo|twitch|dailymotion|soundcloud|bilibili|google drive|drive|mega|mediafire|dropbox|pixeldrain|kick|threads|bandcamp|streamable|pinterest|spotify|watch|video|videos|home|shorts)$/i;
  const siteLabel = () => {
    const parts = host.split('.');
    return (parts.length > 1 ? parts[parts.length - 2] : host).toLowerCase();
  };

  function cleanName(raw) {
    let s = String(raw || '').replace(/\s+/g, ' ').trim();
    s = s.replace(/^\(\d+\+?\)\s*/, '').replace(/\s*#shorts\b/gi, '').trim();
    for (let i = 0; i < 2; i++) {
      for (const sep of [' - ', ' | ', ' • ', ' · ', ' – ', ' — ']) {
        const at = s.lastIndexOf(sep);
        if (at > 0) {
          const tail = s.slice(at + sep.length).trim().toLowerCase();
          if (SITE_WORDS.test(tail) || tail.replace(/[^a-z0-9]/g, '').includes(siteLabel())) s = s.slice(0, at).trim();
        }
      }
    }
    if (!s || SITE_WORDS.test(s) || s.toLowerCase() === siteLabel() || s.toLowerCase() === host) return '';
    return s.slice(0, 200);
  }

  const SITE_TITLES = [
    'ytd-reel-video-renderer[is-active] yt-shorts-video-title-view-model',
    'ytd-reel-video-renderer[is-active] .ytShortsVideoTitleViewModelShortsVideoTitle',
    'ytd-reel-video-renderer[is-active] h2',
    'ytd-watch-metadata h1',
    '#above-the-fold #title h1',
    'shreddit-post[post-title]',
    'article [data-testid="tweetText"]',
    '[data-e2e="browse-video-desc"]',
    '[data-e2e="video-desc"]',
    'h1'
  ];

  function pageName() {
    for (const sel of SITE_TITLES) {
      const el = document.querySelector(sel);
      const value = el?.getAttribute?.('post-title') || text(el);
      const name = cleanName(value);
      if (name) return name;
    }
    return cleanName(document.title) || cleanName(meta('og:title')) || cleanName(meta('twitter:title')) || cleanName(meta('title'));
  }

  function nameFromUrl(url) {
    const u = absolute(url);
    if (!u) return '';
    const last = pathOf(u).split('/').filter(Boolean).pop() || '';
    return /\.[a-z0-9]{2,5}$/i.test(last) ? cleanName(last) : '';
  }

  const remoteTitles = new Map();
  function remoteTitle(url) {
    if (!url) return '';
    if (remoteTitles.has(url)) return remoteTitles.get(url) || '';
    remoteTitles.set(url, '');
    chrome.runtime
      .sendMessage({ type: 'title', url })
      .then((t) => {
        const name = cleanName(t);
        if (name) remoteTitles.set(url, name);
      })
      .catch(() => {});
    return '';
  }

  function resolveName(item, fallbackKind = 'video') {
    const own = cleanName(item?.name);
    if (own) return own;
    const remote = remoteTitle(item?.url);
    if (remote) return remote;
    const here = mediaPage() || filePage() || item?.url === location.href;
    const page = here ? pageName() : '';
    if (page) return page;
    const file = nameFromUrl(item?.url) || nameFromUrl(location.href);
    if (file) return file;
    return pageName() || host + ' ' + fallbackKind;
  }

  function youtubeItem(el) {
    let id = null;
    let name = '';
    const box = el.closest?.('ytd-video-preview, #video-preview') || el.closest?.(YT_CARDS);
    if (box) {
      const link = box.querySelector('a[href*="/watch?v="], a[href*="/shorts/"]');
      id = link ? youtubeId(link.getAttribute('href')) : null;
      name = link?.getAttribute('title') || text(box.querySelector('#video-title, #video-title-link, h3, .yt-lockup-metadata-view-model-wiz__title'));
    }
    if (!id && mediaPage()) {
      id = youtubeId(location.href);
      const shorts = location.pathname.startsWith('/shorts/');
      if (shorts) {
        name = text(document.querySelector('ytd-reel-video-renderer[is-active] yt-shorts-video-title-view-model, ytd-reel-video-renderer[is-active] .ytShortsVideoTitleViewModelShortsVideoTitle, ytd-reel-video-renderer[is-active] h2'));
      } else {
        const flexy = document.querySelector('ytd-watch-flexy');
        const fresh = !flexy?.getAttribute('video-id') || flexy.getAttribute('video-id') === id;
        name = fresh ? text(document.querySelector('ytd-watch-metadata h1, #above-the-fold #title h1, h1.title')) : '';
        if (!name && fresh) name = youtubeTitle(document.title);
      }
      if (/^youtube$/i.test(name)) name = '';
    }
    if (!id) return null;
    return {
      url: location.pathname.startsWith('/shorts/') && !box ? `https://www.youtube.com/shorts/${id}` : `https://www.youtube.com/watch?v=${id}`,
      kind: 'media',
      mediaType: 'video',
      name,
      thumbnail: `https://i.ytimg.com/vi/${id}/hqdefault.jpg`
    };
  }

  function videoItem(el) {
    if (on('youtube.com', 'youtu.be')) {
      const yt = youtubeItem(el);
      if (yt) return yt;
    }
    const reddit = el.matches?.(REDDIT_PLAYER) ? el : el.closest?.(REDDIT_PLAYER);
    if (reddit || on('reddit.com')) {
      const item = redditPlayerItem(reddit || el.closest('shreddit-post')?.querySelector(REDDIT_PLAYER));
      if (item) return item;
    }
    const post = postUrlFor(el);
    if (post) return { url: post, kind: 'media', mediaType: 'video', name: meta('og:title') || document.title };

    const src = el.currentSrc || el.src || el.querySelector?.('source[src]')?.src || '';
    if (/^https?:/i.test(src) && !mediaPage()) {
      const u = absolute(src);
      return { url: src, kind: 'auto', mediaType: 'video', name: u ? pathOf(u).split('/').pop() : document.title };
    }
    return { url: location.href, kind: 'media', mediaType: 'video', name: meta('og:title') || document.title, thumbnail: meta('og:image') };
  }

  const ICON =
    '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M12 4v12m0 0l-5-5m5 5l5-5M5 20h14"/></svg>';

  function sendItem(item, quality) {
    send({
      url: item.url,
      kind: item.kind,
      quality: item.kind === 'media' ? quality : undefined,
      filename: item.filename || null,
      pageTitle: resolveName(item),
      thumbnail: item.thumbnail || meta('og:image'),
      referrer: location.href
    });
  }

  function qualityButton(label, onPick) {
    const wrap = document.createElement('div');
    const main = document.createElement('button');
    main.className = 'multron-btn';
    main.innerHTML = `${ICON}<span>${label}</span>`;
    const menu = document.createElement('div');
    menu.className = 'multron-menu';
    for (const [id, label] of QUALITIES) {
      const item = document.createElement('button');
      item.textContent = label;
      item.addEventListener('click', (e) => {
        e.preventDefault();
        e.stopPropagation();
        menu.classList.remove('open');
        onPick(id);
      });
      menu.appendChild(item);
    }
    main.addEventListener('click', (e) => {
      e.preventDefault();
      e.stopPropagation();
      menu.classList.toggle('open');
    });
    document.addEventListener('click', () => menu.classList.remove('open'));
    wrap.append(main, menu);
    return wrap;
  }

  let fab = null;
  let fabKind = null;

  function showFab(kind) {
    if (fab && fabKind === kind) return;
    hideFab();
    fabKind = kind;
    fab = document.createElement('div');
    fab.className = 'multron-fab';
    if (kind === 'media') {
      fab.appendChild(qualityButton('Multron Download Manager', (q) => sendItem(videoItem(document.body), q)));
    } else {
      const main = document.createElement('button');
      main.className = 'multron-btn';
      main.innerHTML = `${ICON}<span>Multron Download Manager</span>`;
      main.addEventListener('click', () => send({ url: location.href, kind: 'http', pageTitle: resolveName({ url: location.href }, 'file') }));
      fab.appendChild(main);
    }
    document.documentElement.appendChild(fab);
    updateFabLabel();
  }

  function updateFabLabel() {
    if (!fab) return;
    const name = fabKind === 'media' ? resolveName(videoItem(document.body)) : resolveName({ url: location.href }, 'file');
    const label = fab.querySelector('.multron-btn span');
    const wanted = `Download: ${name}`;
    if (label && label.textContent !== wanted) {
      label.textContent = wanted;
      label.parentElement.title = name;
    }
  }

  function hideFab() {
    fab?.remove();
    fab = null;
    fabKind = null;
  }

  let overlay = null;
  let overlayTarget = null;

  function visibleRect(el) {
    const r = el.getBoundingClientRect();
    if (r.width < 240 || r.height < 135 || r.bottom < 0 || r.top > innerHeight) return null;
    const style = getComputedStyle(el);
    if (style.visibility === 'hidden' || style.display === 'none' || Number(style.opacity) === 0) return null;
    return r;
  }

  function mainVideo() {
    let best = null;
    let bestArea = 0;
    const candidates = [...document.querySelectorAll('video'), ...document.querySelectorAll(REDDIT_PLAYER)];
    for (const el of candidates) {
      if (el.matches(REDDIT_PLAYER) && el.hasAttribute('post-promoted')) continue;
      const r = visibleRect(el);
      if (!r) continue;
      const area = r.width * r.height;
      if (area > bestArea) {
        best = el;
        bestArea = area;
      }
    }
    return best;
  }

  function placeOverlay() {
    if (!overlay || !overlayTarget) return;
    const r = overlayTarget.getBoundingClientRect();
    overlay.style.top = `${Math.max(8, r.top + 12)}px`;
    overlay.style.left = `${Math.min(innerWidth - 8, r.right - 12)}px`;
  }

  function updateOverlay() {
    const target = enabled ? mainVideo() : null;
    if (!target || document.fullscreenElement) {
      overlay?.remove();
      overlay = null;
      overlayTarget = null;
      return;
    }
    if (!overlay) {
      overlay = document.createElement('div');
      overlay.className = 'multron-video';
      overlay.appendChild(qualityButton('Download video', (q) => overlayTarget && sendItem(videoItem(overlayTarget), q)));
      document.documentElement.appendChild(overlay);
    }
    overlayTarget = target;
    const label = overlay.querySelector('.multron-btn span');
    const name = resolveName(videoItem(target));
    const wanted = `Download: ${name}`;
    if (label && label.textContent !== wanted) {
      label.textContent = wanted;
      label.parentElement.title = name;
    }
    placeOverlay();
  }

  let enabled = true;
  function refresh() {
    updateOverlay();
    const kind = !enabled ? null : filePage() ? 'file' : mediaPage() && !overlay ? 'media' : null;
    if (kind) {
      showFab(kind);
      updateFabLabel();
    } else hideFab();
  }

  addEventListener('scroll', placeOverlay, { passive: true, capture: true });
  addEventListener('resize', placeOverlay, { passive: true });

  function bestImage(img) {
    const set = img.getAttribute('srcset');
    if (set) {
      let best = null;
      let bestW = 0;
      for (const part of set.split(',')) {
        const [url, size] = part.trim().split(/\s+/);
        const w = parseFloat(size) || 1;
        if (w >= bestW) {
          best = url;
          bestW = w;
        }
      }
      if (best) return absolute(best)?.href ?? null;
    }
    return absolute(img.currentSrc || img.src)?.href ?? null;
  }

  function fullSize(url) {
    try {
      const u = new URL(url);
      if (u.hostname === 'preview.redd.it' || u.hostname === 'i.redd.it') {
        const file = u.pathname.split('/').pop().replace(/^.*-v0-/, '');
        return `https://i.redd.it/${file}`;
      }
      if (u.hostname === 'pbs.twimg.com' && u.pathname.startsWith('/media/')) {
        u.searchParams.set('name', 'orig');
        return u.href;
      }
      return url;
    } catch {
      return url;
    }
  }

  const fileName = (url) => {
    const u = absolute(url);
    return u ? pathOf(u).split('/').filter(Boolean).pop() || u.hostname : url;
  };

  function siteItems(add) {
    if (on('reddit.com')) {
      for (const post of document.querySelectorAll('shreddit-post')) {
        if (post.hasAttribute('promoted') || post.closest('shreddit-ad-post')) continue;
        const title = post.getAttribute('post-title') || 'Reddit post';
        const type = post.getAttribute('post-type');
        const player = redditPlayerItem(post.querySelector(REDDIT_PLAYER));
        if (player) add(player);
        else if (type === 'image') {
          const href = post.getAttribute('content-href');
          if (href) add({ url: fullSize(href), kind: 'auto', mediaType: 'image', name: title });
        } else if (type === 'gallery') {
          const urls = new Set();
          for (const img of post.querySelectorAll('gallery-carousel img, ul img, img[src*="preview.redd.it"]')) {
            const src = bestImage(img);
            if (src && /redd\.it/.test(src)) urls.add(fullSize(src));
          }
          [...urls].forEach((url, i) => add({ url, kind: 'auto', mediaType: 'image', name: `${title} (${i + 1})` }));
        } else if (type === 'link') {
          const href = post.getAttribute('content-href');
          if (href && /youtube\.com|youtu\.be|streamable|vimeo|gfycat|redgifs|imgur|v\.redd\.it/.test(href)) {
            add({ url: redditVideo(href), kind: 'media', mediaType: 'video', name: title, filename: /v\.redd\.it/.test(href) ? title : undefined });
          }
        }
      }
      redditCommentItems(add);
      return;
    }
    if (on('instagram.com')) {
      for (const a of document.querySelectorAll('a[href*="/p/"], a[href*="/reel/"], a[href*="/tv/"]')) {
        const u = absolute(a.getAttribute('href'));
        const m = u && u.pathname.match(/\/(p|reel|tv)\/([\w-]+)/);
        if (!m) continue;
        const img = a.querySelector('img');
        add({
          url: `https://www.instagram.com/${m[1]}/${m[2]}/`,
          kind: 'media',
          mediaType: m[1] === 'p' ? 'post' : 'video',
          name: img?.alt?.slice(0, 120) || `Instagram ${m[2]}`,
          thumbnail: img ? bestImage(img) : null
        });
      }
      return;
    }
    if (on('x.com', 'twitter.com')) {
      for (const article of document.querySelectorAll('article')) {
        const link = article.querySelector('a[href*="/status/"]:has(time)') || article.querySelector('a[href*="/status/"]');
        const url = link && absolute(link.getAttribute('href'))?.href;
        const name = text(article.querySelector('[data-testid="tweetText"]')).slice(0, 120) || 'Post';
        if (url && article.querySelector('[data-testid="videoPlayer"], video')) add({ url, kind: 'media', mediaType: 'video', name });
        for (const img of article.querySelectorAll('img[src*="pbs.twimg.com/media"]')) {
          add({ url: fullSize(img.src), kind: 'auto', mediaType: 'image', name, thumbnail: img.src });
        }
      }
      return;
    }
    if (on('youtube.com')) {
      for (const a of document.querySelectorAll('a[href*="/watch?v="], a[href^="/shorts/"]')) {
        const u = absolute(a.getAttribute('href'));
        if (!u) continue;
        const id = u.searchParams.get('v') || u.pathname.split('/shorts/')[1]?.split(/[/?]/)[0];
        if (!id) continue;
        const card = a.closest('ytd-rich-item-renderer, ytd-video-renderer, ytd-compact-video-renderer, ytd-reel-item-renderer, ytd-grid-video-renderer, ytd-playlist-video-renderer');
        const name = a.getAttribute('title') || text(card?.querySelector('#video-title')) || text(a);
        if (!name) continue;
        add({
          url: `https://www.youtube.com/watch?v=${id}`,
          kind: 'media',
          mediaType: 'video',
          name,
          thumbnail: `https://i.ytimg.com/vi/${id}/mqdefault.jpg`
        });
      }
      return;
    }
    if (on('tiktok.com')) {
      for (const a of document.querySelectorAll('a[href*="/video/"]')) {
        const u = absolute(a.getAttribute('href'));
        if (u && /\/video\/\d+/.test(u.pathname)) {
          add({ url: u.origin + u.pathname, kind: 'media', mediaType: 'video', name: a.querySelector('img')?.alt || `TikTok ${u.pathname.split('/').pop()}` });
        }
      }
      return;
    }
    if (on('facebook.com')) {
      for (const a of document.querySelectorAll('a[href*="/videos/"], a[href*="/reel/"], a[href*="/watch/?v="]')) {
        const u = absolute(a.getAttribute('href'));
        if (u) add({ url: u.href.split('&')[0], kind: 'media', mediaType: 'video', name: text(a) || 'Facebook video' });
      }
    }
  }

  function genericItems(add) {
    if (mediaPage() && !on('reddit.com')) add({ url: location.href, kind: 'media', mediaType: 'video', name: meta('og:title') || document.title, thumbnail: meta('og:image') });

    for (const v of document.querySelectorAll('video, audio')) {
      const srcs = [v.currentSrc, v.src, ...[...v.querySelectorAll('source[src]')].map((s) => s.src)];
      for (const src of srcs) {
        if (src && /^https?:/i.test(src) && !/\.m3u8|\.mpd/i.test(src)) {
          add({ url: src, kind: 'auto', mediaType: v.tagName === 'AUDIO' ? 'audio' : 'video', name: fileName(src), thumbnail: v.poster || null });
        }
      }
    }
    for (const a of document.querySelectorAll('a[href]')) {
      const u = absolute(a.getAttribute('href'));
      if (!u) continue;
      const p = pathOf(u);
      if (a.href.startsWith('magnet:')) continue;
      if (IMAGE_EXT.test(p)) add({ url: u.href, kind: 'auto', mediaType: 'image', name: fileName(u.href), thumbnail: a.querySelector('img') ? bestImage(a.querySelector('img')) : u.href });
      else if (VIDEO_EXT.test(p)) add({ url: u.href, kind: 'auto', mediaType: 'video', name: fileName(u.href) });
      else if (AUDIO_EXT.test(p)) add({ url: u.href, kind: 'auto', mediaType: 'audio', name: fileName(u.href) });
      else if (FILE_EXT.test(p) || a.hasAttribute('download')) add({ url: u.href, kind: /\.torrent$/i.test(p) ? 'torrent' : 'auto', mediaType: 'file', name: a.getAttribute('download') || fileName(u.href) });
    }
    for (const a of document.querySelectorAll('a[href^="magnet:"]')) {
      add({ url: a.href, kind: 'torrent', mediaType: 'file', name: text(a) || 'Magnet link' });
    }
    if (on('reddit.com', 'instagram.com', 'x.com', 'twitter.com', 'youtube.com', 'tiktok.com')) return;
    for (const img of document.querySelectorAll('img')) {
      if ((img.naturalWidth || img.width) < 300 || (img.naturalHeight || img.height) < 200) continue;
      const src = bestImage(img);
      if (!src || src.startsWith('data:') || /\.svg(\?|$)/i.test(src)) continue;
      add({ url: fullSize(src), kind: 'auto', mediaType: 'image', name: img.alt?.slice(0, 120) || fileName(src), thumbnail: src });
    }
  }

  function collect(found) {
    const add = (item) => {
      if (!item?.url || found.has(item.url) || found.size >= 2000) return;
      found.set(item.url, { referrer: location.href, ...item });
    };
    siteItems(add);
    genericItems(add);
  }

  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

  async function grab(pages) {
    const found = new Map();
    collect(found);
    if (pages > 0) {
      const start = scrollY;
      let still = 0;
      for (let i = 0; i < pages && still < 3; i++) {
        const before = document.documentElement.scrollHeight;
        scrollBy({ top: innerHeight * 0.9 });
        await sleep(1200);
        collect(found);
        still = document.documentElement.scrollHeight === before && innerHeight + scrollY >= before - 4 ? still + 1 : 0;
      }
      scrollTo({ top: start });
    }
    return {
      pageUrl: location.href,
      pageTitle: meta('og:title') || document.title,
      items: [...found.values()]
    };
  }

  chrome.runtime.onMessage.addListener((msg, _sender, reply) => {
    if (msg?.type !== 'grab') return;
    grab(Number(msg.pages) || 0).then(reply);
    return true;
  });

  chrome.runtime.sendMessage({ type: 'settings' }).then((s) => {
    enabled = s?.mediaButton !== false;
    refresh();
    setInterval(refresh, 1000);

    const skip = String(s?.skipDomains || '')
      .split(/[\s,]+/)
      .map((d) => d.trim().toLowerCase())
      .filter(Boolean);
    if (s?.intercept !== false && !skip.some((d) => on(d))) {
      document.addEventListener(
        'click',
        (e) => {
          if (e.button !== 0 || e.defaultPrevented || e.ctrlKey || e.metaKey || e.shiftKey || e.altKey) return;
          const a = e.target instanceof Element ? e.target.closest('a[href]') : null;
          if (!a || !isFileLink(a)) return;
          e.preventDefault();
          e.stopPropagation();
          chrome.runtime
            .sendMessage({
              type: 'link-download',
              payload: { url: a.href, kind: 'auto', filename: a.getAttribute('download') || null, pageTitle: document.title }
            })
            .then((r) => toast(r === 'browser' ? 'Multron Download Manager is not running: downloading in the browser' : r ? 'Sent to Multron Download Manager' : 'Multron Download Manager did not accept the download', !!r));
        },
        true
      );
    }

    if (s?.interceptMagnets !== false) {
      document.addEventListener(
        'click',
        (e) => {
          const a = e.target instanceof Element ? e.target.closest('a[href^="magnet:"]') : null;
          if (!a || e.button !== 0) return;
          e.preventDefault();
          e.stopPropagation();
          send({ url: a.href, kind: 'torrent', pageTitle: a.textContent?.trim() || null });
        },
        true
      );
    }
  });
})();
