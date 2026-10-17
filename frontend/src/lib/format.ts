const UNITS = ['B', 'KB', 'MB', 'GB', 'TB'];

export function bytes(n: number | null | undefined, digits = 1): string {
  if (n == null || !isFinite(n)) return '—';
  let v = n;
  let i = 0;
  while (v >= 1024 && i < UNITS.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(i === 0 ? 0 : digits)} ${UNITS[i]}`;
}

export function speed(n: number | null | undefined): string {
  if (!n) return '0 KB/s';
  return `${bytes(n)}/s`;
}

export function duration(sec: number | null | undefined): string {
  if (sec == null || !isFinite(sec)) return '—';
  const s = Math.max(0, Math.round(sec));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = s % 60;
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${r}s`;
  return `${r}s`;
}

export function percent(done: number, total: number | null | undefined): number {
  if (!total) return 0;
  return Math.min(100, Math.max(0, (done / total) * 100));
}

export function date(ms: number | null | undefined, lang = 'en'): string {
  if (!ms) return '—';
  return new Date(ms).toLocaleString(lang === 'tr' ? 'tr-TR' : 'en-US');
}

export function extension(name: string): string {
  const i = name.lastIndexOf('.');
  return i > 0 ? name.slice(i + 1).toLowerCase() : '';
}

export function decodeBits(b64: string, count: number): boolean[] {
  const raw = atob(b64);
  const out: boolean[] = new Array(count);
  for (let i = 0; i < count; i++) {
    const byte = raw.charCodeAt(i >> 3);
    out[i] = ((byte >> (7 - (i & 7))) & 1) === 1;
  }
  return out;
}
