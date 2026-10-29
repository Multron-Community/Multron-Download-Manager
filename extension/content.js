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
    if (on('instagram.com')) return /^\/(p|reel|reels|tv)\//.test(p) || /^\/[^/]+\/(p|reel)\//.test(p) || /^\/stories\/(highlights\/)?[^/]+\/?(\d+\/?)?$/.test(p);
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
    ['6480', '12K'],
    ['4320', '8K'],
    ['2880', '5K'],
    ['2160', '4K'],
    ['1440', '1440p'],
    ['1080', '1080p'],
    ['720', '720p'],
    ['480', '480p'],
    ['360', '360p'],
    ['240', '240p'],
    ['144', '144p'],
    ['audio-mp3', 'Audio (MP3)'],
    ['audio-m4a', 'Audio (M4A)'],
    ['audio-aac', 'Audio (AAC)'],
    ['audio-opus', 'Audio (Opus)'],
    ['audio-vorbis', 'Audio (OGG Vorbis)'],
    ['audio-flac', 'Audio (FLAC)'],
    ['audio-wav', 'Audio (WAV)'],
    ['audio-alac', 'Audio (ALAC)']
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

  function storyStreams(duration) {
    const tracks = [];
    for (const e of performance.getEntriesByType('resource')) {
      let u;
      try {
        u = new URL(e.name);
      } catch {
        continue;
      }
      const efg = u.searchParams.get('efg');
      if (!efg) continue;
      let info;
      try {
        info = JSON.parse(atob(efg.replace(/-/g, '+').replace(/_/g, '/')));
      } catch {
        continue;
      }
      const tag = String(info.vencode_tag || '');
      if (!tag.includes('story')) continue;
      u.searchParams.delete('bytestart');
      u.searchParams.delete('byteend');
      tracks.push({
        url: u.href,
        audio: /audio/i.test(tag),
        asset: String(info.xpv_asset_id ?? ''),
        duration: Number(info.duration_s),
        bitrate: Number(info.bitrate) || 0,
        at: e.startTime
      });
    }
    const exact = (t) => t.duration === Math.floor(duration);
    const near = (t) => Number.isFinite(t.duration) && Math.abs(t.duration - duration) < 1;
    const fits = tracks.some((t) => !t.audio && exact(t)) ? exact : near;
    const video = tracks
      .filter((t) => !t.audio && fits(t))
      .sort((a, b) => b.at - a.at || b.bitrate - a.bitrate)[0];
    if (!video) return null;
    const sameAsset = (t) => video.asset && t.asset === video.asset;
    const audios = tracks.filter((t) => t.audio && fits(t));
    const audio =
      audios.find(sameAsset) ||
      audios.sort((a, b) => Math.abs(a.at - video.at) - Math.abs(b.at - video.at))[0];
    const best = tracks
      .filter((t) => !t.audio && (sameAsset(t) || t.url === video.url))
      .sort((a, b) => b.bitrate - a.bitrate)[0];
    return { video: best.url, audio: audio?.url || null };
  }

  function storyItem(el) {
    if (!on('instagram.com') || !location.pathname.startsWith('/stories/')) return null;
    const videos = [...document.querySelectorAll('video')];
    const video = el?.tagName === 'VIDEO' ? el : videos.find((v) => !v.paused) || videos[0];
    if (!video) return null;
    const parts = location.pathname.split('/').filter(Boolean);
    const owner = parts[1] === 'highlights' ? 'highlight' : parts[1] || 'instagram';
    const id = parts[2] || String(Date.now());
    const name = `${owner}_story_${id}.mp4`;
    const src = [video.currentSrc, video.src].find((x) => x && /^https?:/i.test(x));
    if (src) return { url: src, kind: 'auto', mediaType: 'video', name };
    const streams = storyStreams(video.duration);
    if (!streams) return null;
    return { url: streams.video, audioUrl: streams.audio, kind: 'media', mediaType: 'video', name, filename: name };
  }

  function videoItem(el) {
    const story = storyItem(el);
    if (story) return story;
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

  const FORMATS = [
    ['default', 'Default'],
    ['mp4', 'MP4'],
    ['mp4-h264', 'MP4 H.264'],
    ['mkv', 'MKV'],
    ['webm', 'WebM'],
    ['mov', 'MOV'],
    ['avi', 'AVI'],
    ['flv', 'FLV']
  ];
  let videoFormat = 'default';
  const formatPickers = new Set();
  chrome.storage.local
    .get('videoFormat')
    .then((v) => {
      videoFormat = v?.videoFormat || 'default';
      formatPickers.forEach((paint) => paint());
    })
    .catch(() => {});

  function formatRow() {
    const row = document.createElement('div');
    row.className = 'multron-formats';
    const chips = FORMATS.map(([id, label]) => {
      const chip = document.createElement('span');
      chip.className = 'multron-chip';
      chip.textContent = label;
      chip.title =
        id === 'default'
          ? 'Use the video format set in Multron Download Manager'
          : id === 'mp4-h264'
            ? 'MP4 with H.264 and AAC: plays everywhere, also in WhatsApp and on older phones and TVs (up to 1080p on YouTube)'
            : `Save videos as ${label}`;
      chip.addEventListener('click', (e) => {
        e.preventDefault();
        e.stopPropagation();
        videoFormat = id;
        chrome.storage.local.set({ videoFormat: id }).catch(() => {});
        formatPickers.forEach((paint) => paint());
      });
      row.appendChild(chip);
      return [id, chip];
    });
    const paint = () => chips.forEach(([id, chip]) => chip.classList.toggle('on', id === videoFormat));
    formatPickers.add(paint);
    paint();
    return row;
  }

  function sendItem(item, quality) {
    const audio = String(quality || '').startsWith('audio');
    send({
      url: item.url,
      kind: item.kind,
      quality: item.kind === 'media' ? quality : undefined,
      container: item.kind === 'media' && !audio && videoFormat !== 'default' ? videoFormat : undefined,
      filename: item.filename || null,
      audioUrl: item.audioUrl || undefined,
      pageTitle: resolveName(item),
      thumbnail: item.thumbnail || meta('og:image'),
      referrer: location.href
    });
  }

  function sizeText(s) {
    if (!s?.size) return '';
    const gb = s.size / 1073741824;
    const text = gb >= 1 ? `${gb.toFixed(gb >= 10 ? 0 : 1)} GB` : `${Math.max(0.1, s.size / 1048576).toFixed(1)} MB`;
    return (s.exact ? '' : '~') + text;
  }

  function qualityButton(label, onPick, itemOf) {
    const wrap = document.createElement('div');
    const main = document.createElement('button');
    main.className = 'multron-btn';
    main.innerHTML = `${ICON}<span>${label}</span>`;
    const menu = document.createElement('div');
    menu.className = 'multron-menu';
    const status = document.createElement('div');
    status.className = 'multron-menu-status';
    status.hidden = true;
    menu.appendChild(status);
    menu.appendChild(formatRow());
    const sizeCells = new Map();
    let sizesFor = null;
    const showStatus = (text, busy) => {
      status.hidden = !text;
      status.classList.toggle('busy', !!busy);
      status.textContent = text || '';
    };
    const fill = (sizes) => {
      for (const [id, cell] of sizeCells) {
        cell.classList.remove('loading');
        cell.textContent = sizeText(sizes?.[id]);
      }
    };
    const loadSizes = () => {
      const item = itemOf?.();
      const url = item?.kind === 'media' ? item.url : null;
      if (!url || sizesFor === url) return;
      sizesFor = url;
      showStatus('Checking sizes…', true);
      for (const cell of sizeCells.values()) {
        cell.textContent = '';
        cell.classList.add('loading');
      }
      chrome.runtime
        .sendMessage({ type: 'sizes', url })
        .then((sizes) => {
          if (sizesFor !== url) return;
          fill(sizes);
          showStatus(sizes ? '' : 'Sizes are not available for this video', false);
          if (!sizes) sizesFor = null;
        })
        .catch(() => {
          fill(null);
          showStatus('Sizes are not available for this video', false);
          sizesFor = null;
        });
    };
    for (const [id, label] of QUALITIES) {
      const item = document.createElement('button');
      const name = document.createElement('span');
      name.textContent = label;
      const size = document.createElement('span');
      size.className = 'multron-size';
      sizeCells.set(id, size);
      item.append(name, size);
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
      if (menu.classList.toggle('open')) loadSizes();
    });
    document.addEventListener('click', () => menu.classList.remove('open'));
    wrap.append(main, menu);
    return wrap;
  }

  let dismissed = null;

  function closeButton() {
    const close = document.createElement('button');
    close.className = 'multron-close';
    close.type = 'button';
    close.title = 'Hide on this page';
    close.setAttribute('aria-label', 'Hide on this page');
    close.textContent = '×';
    close.addEventListener('click', (e) => {
      e.preventDefault();
      e.stopPropagation();
      dismissed = location.href;
      refresh();
    });
    return close;
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
      fab.appendChild(qualityButton('Multron Download Manager', (q) => sendItem(videoItem(document.body), q), () => videoItem(document.body)));
    } else {
      const main = document.createElement('button');
      main.className = 'multron-btn';
      main.innerHTML = `${ICON}<span>Multron Download Manager</span>`;
      main.addEventListener('click', () => send({ url: location.href, kind: 'http', pageTitle: resolveName({ url: location.href }, 'file') }));
      fab.appendChild(main);
    }
    fab.appendChild(closeButton());
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

  const PLACE_KEY = 'overlayPlace';
  let overlayPlace = null;
  chrome.storage.local
    .get(PLACE_KEY)
    .then((v) => {
      overlayPlace = v?.[PLACE_KEY]?.[location.hostname] || null;
      placeOverlay();
    })
    .catch(() => {});

  function savePlace() {
    chrome.storage.local
      .get(PLACE_KEY)
      .then((v) => {
        const all = { ...(v?.[PLACE_KEY] || {}) };
        if (overlayPlace) all[location.hostname] = overlayPlace;
        else delete all[location.hostname];
        return chrome.storage.local.set({ [PLACE_KEY]: all });
      })
      .catch(() => {});
  }

  function placeOverlay() {
    if (!overlay || !overlayTarget) return;
    const r = overlayTarget.getBoundingClientRect();
    const x = overlayPlace ? r.left + overlayPlace.x * r.width : r.right - 12;
    const y = overlayPlace ? r.top + overlayPlace.y * r.height : r.top + 12;
    const w = overlay.offsetWidth || 0;
    overlay.style.top = `${Math.min(innerHeight - 40, Math.max(8, y))}px`;
    overlay.style.left = `${Math.min(innerWidth - 8, Math.max(8 + w, x))}px`;
  }

  function makeDraggable(handle) {
    let start = null;
    let moved = false;
    handle.addEventListener('pointerdown', (e) => {
      if (e.button !== 0 || !overlay) return;
      const box = overlay.getBoundingClientRect();
      start = { x: e.clientX, y: e.clientY, right: box.right, top: box.top };
      moved = false;
      handle.setPointerCapture?.(e.pointerId);
    });
    handle.addEventListener('pointermove', (e) => {
      if (!start || !overlay || !overlayTarget) return;
      const dx = e.clientX - start.x;
      const dy = e.clientY - start.y;
      if (!moved && Math.hypot(dx, dy) < 5) return;
      if (!moved) {
        moved = true;
        overlay.classList.add('multron-dragging');
        overlay.querySelector('.multron-menu')?.classList.remove('open');
      }
      const r = overlayTarget.getBoundingClientRect();
      const x = Math.min(innerWidth - 8, Math.max(8 + overlay.offsetWidth, start.right + dx));
      const y = Math.min(innerHeight - overlay.offsetHeight - 8, Math.max(8, start.top + dy));
      overlayPlace = { x: (x - r.left) / Math.max(1, r.width), y: (y - r.top) / Math.max(1, r.height) };
      placeOverlay();
    });
    const end = () => {
      if (!start) return;
      start = null;
      overlay?.classList.remove('multron-dragging');
      if (moved) savePlace();
    };
    handle.addEventListener('pointerup', end);
    handle.addEventListener('pointercancel', end);
    handle.addEventListener(
      'click',
      (e) => {
        if (!moved) return;
        moved = false;
        e.preventDefault();
        e.stopImmediatePropagation();
      },
      true
    );
    handle.addEventListener('dblclick', (e) => {
      e.preventDefault();
      e.stopPropagation();
      overlayPlace = null;
      overlay?.querySelector('.multron-menu')?.classList.remove('open');
      savePlace();
      placeOverlay();
    });
  }

  function updateOverlay() {
    const target = enabled && dismissed !== location.href ? mainVideo() : null;
    if (!target || document.fullscreenElement) {
      overlay?.remove();
      overlay = null;
      overlayTarget = null;
      return;
    }
    if (!overlay) {
      overlay = document.createElement('div');
      overlay.className = 'multron-video';
      overlay.appendChild(qualityButton('Download video', (q) => overlayTarget && sendItem(videoItem(overlayTarget), q), () => overlayTarget && videoItem(overlayTarget)));
      overlay.appendChild(closeButton());
      document.documentElement.appendChild(overlay);
      const handle = overlay.querySelector('.multron-btn');
      handle.title = 'Drag to move · double-click to put it back';
      makeDraggable(handle);
    }
    overlayTarget = target;
    const label = overlay.querySelector('.multron-btn span');
    const name = resolveName(videoItem(target));
    const wanted = `Download: ${name}`;
    if (label && label.textContent !== wanted) {
      label.textContent = wanted;
      label.parentElement.title = `${name}\nDrag to move · double-click to put it back`;
    }
    placeOverlay();
  }

  const IDLE_MS = 2500;
  let mini = null;
  let miniTarget = null;
  let miniTimer = 0;

  function fullscreenHost() {
    const host = document.fullscreenElement;
    return host && host.tagName !== 'VIDEO' ? host : null;
  }

  function fullscreenVideo(host) {
    if (host.matches?.(REDDIT_PLAYER)) return host;
    const videos = [...host.querySelectorAll('video')];
    return videos.find((v) => !v.paused) || videos[0] || null;
  }

  function wakeMini() {
    if (!mini) return;
    mini.classList.remove('multron-idle');
    clearTimeout(miniTimer);
    miniTimer = setTimeout(() => {
      if (mini && !mini.matches(':hover') && !mini.querySelector('.multron-menu.open')) mini.classList.add('multron-idle');
    }, IDLE_MS);
  }

  function updateMini() {
    const host = enabled && fullscreenEnabled && dismissed !== location.href && miniDismissed !== location.href ? fullscreenHost() : null;
    miniTarget = host ? fullscreenVideo(host) : null;
    if (!host || !miniTarget) {
      clearTimeout(miniTimer);
      mini?.remove();
      mini = null;
      return;
    }
    if (!mini) {
      mini = document.createElement('div');
      mini.className = 'multron-mini';
      mini.appendChild(qualityButton('', (q) => miniTarget && sendItem(videoItem(miniTarget), q), () => miniTarget && videoItem(miniTarget)));
      const button = mini.querySelector('.multron-btn');
      button.title = 'Download with Multron Download Manager';
      button.setAttribute('aria-label', 'Download with Multron Download Manager');
      const close = document.createElement('button');
      close.className = 'multron-mini-close';
      close.type = 'button';
      close.title = 'Hide in fullscreen on this page (turn it off everywhere in Multron Download Manager: Settings → Browser Integration)';
      close.setAttribute('aria-label', 'Hide in fullscreen on this page');
      close.textContent = '×';
      close.addEventListener('click', (e) => {
        e.preventDefault();
        e.stopPropagation();
        miniDismissed = location.href;
        refresh();
      });
      mini.appendChild(close);
      mini.addEventListener('mouseleave', wakeMini);
    }
    if (mini.parentElement !== host) {
      host.appendChild(mini);
      wakeMini();
    }
  }

  for (const type of ['mousemove', 'pointerdown', 'keydown', 'touchstart']) {
    addEventListener(type, wakeMini, { passive: true, capture: true });
  }
  document.addEventListener('fullscreenchange', () => refresh());

  let enabled = true;
  let fullscreenEnabled = true;
  let miniDismissed = null;
  function refresh() {
    updateMini();
    updateOverlay();
    const kind = !enabled || dismissed === location.href || document.fullscreenElement ? null : filePage() ? 'file' : mediaPage() && !overlay ? 'media' : null;
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
    fullscreenEnabled = s?.fullscreenButton !== false;
    refresh();
    chrome.storage.onChanged.addListener((changes, area) => {
      if (area !== 'local') return;
      if (changes.mediaButton) enabled = changes.mediaButton.newValue !== false;
      if (changes.fullscreenButton) fullscreenEnabled = changes.fullscreenButton.newValue !== false;
      if (changes.mediaButton || changes.fullscreenButton) refresh();
    });
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
