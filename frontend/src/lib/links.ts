import type { PageBatch } from '../types';

const MAX_LINKS = 1000;

export function expandRange(line: string): string[] {
  const m = line.match(/\[(\d+)-(\d+)\]/);
  if (!m) return [line];
  const [from, to] = [m[1], m[2]];
  const start = parseInt(from, 10);
  const end = parseInt(to, 10);
  if (end < start || end - start >= MAX_LINKS) return [line];
  const width = from.length === to.length ? from.length : 0;
  const out: string[] = [];
  for (let n = start; n <= end; n++) {
    const num = width ? String(n).padStart(width, '0') : String(n);
    out.push(...expandRange(line.slice(0, m.index) + num + line.slice((m.index ?? 0) + m[0].length)));
    if (out.length >= MAX_LINKS) break;
  }
  return out.slice(0, MAX_LINKS);
}

export function linksFromText(text: string): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const raw of text.split(/[\s"<>]+/)) {
    const word = raw.replace(/^[(,;']+|[),;'.]+$/g, '');
    if (!/^(https?:\/\/|magnet:\?)/i.test(word)) continue;
    for (const link of expandRange(word)) {
      if (!seen.has(link)) {
        seen.add(link);
        out.push(link);
      }
      if (out.length >= MAX_LINKS) return out;
    }
  }
  return out;
}

function nameOf(url: string): string | null {
  if (!/^https?:/i.test(url)) return null;
  try {
    const last = new URL(url).pathname.split('/').filter(Boolean).pop();
    return last ? decodeURIComponent(last) : null;
  } catch {
    return null;
  }
}

export function linkBatch(links: string[], title: string): PageBatch {
  return {
    pageUrl: '',
    pageTitle: title,
    userAgent: null,
    cookies: [],
    items: links.map((url) => ({
      url,
      kind: url.toLowerCase().startsWith('magnet:') ? 'torrent' : 'auto',
      mediaType: 'file',
      name: nameOf(url),
      filename: null,
      thumbnail: null,
      referrer: null
    }))
  };
}
