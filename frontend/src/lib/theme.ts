
import { api } from './api';

export type ThemePref = 'system' | 'dark' | 'light';
export type ThemeName = 'dark' | 'light';

export interface Appearance {
  theme: string;
  accentColor: string;
  accentCustom?: string;
  glassEffect?: boolean;
  accentContrast?: 'auto' | 'exact';
  accentText?: string;
  buttonColor?: string;
  hoverGlow?: boolean;
  clickRipple?: boolean;
  stateGlow?: boolean;
  motion?: 'on' | 'system' | 'off';
  effectsOff?: string[];
  headingColor?: string;
  lightBrightness?: number;
  darkBrightness?: number;
  wallpaper?: boolean;
  wallpaperOpacity?: number;
  backgroundImage?: string;
  backgroundOpacity?: number;
  backgroundBlur?: number;
  roundedCorners?: boolean;
  transparency: boolean;
  backdropMaterial: string;
  transparencyLevel: number;
}

export const ACCENTS: { id: string; name: string; color: string }[] = [
  { id: 'blue', name: 'Blue', color: '#2f8cf0' },
  { id: 'cyan', name: 'Cyan', color: '#4fc3d1' },
  { id: 'green', name: 'Green', color: '#2eb85c' },
  { id: 'orange', name: 'Orange', color: '#ff8c1a' },
  { id: 'purple', name: 'Purple', color: '#8e5ce6' },
  { id: 'red', name: 'Red', color: '#e53935' },
  { id: 'pink', name: 'Pink', color: '#e84a9b' },
  { id: 'teal', name: 'Teal', color: '#14b8a6' },
  { id: 'gray', name: 'Gray', color: '#808080' }
];

export const MATERIALS: Record<string, string> = {
  mica: 'Mica',
  acrylic: 'Acrylic',
  blur: 'Blur',
  vibrancy: 'Vibrancy'
};

type Palette = Record<string, string>;

const SURFACES = ['bg', 'panel', 'raised', 'hover', 'subtle'] as const;

const PALETTES: Record<ThemeName, Palette> = {
  dark: {
    bg: '#0e131c',
    panel: '#131a25',
    raised: '#18202d',
    hover: '#1d2635',
    subtle: '#111720',
    line: '#242e3e',
    'card-line': '#1e2736',
    'row-line': '#1e2736',
    'row-alt': '#10161f',
    'btn-hover': '#2a3548',
    white: '#e9edf3',
    'on-primary': '#0e131c',
    menu: '#171f2c',
    'fg-soft': '#dbe2eb',
    'primary-hover': '#e2e8f0',
    muted: '#98a2b3',
    dim: '#5e6a7d',
    ok: '#34c759',
    warn: '#ffb340',
    bad: '#ff5a52',
    'bad-soft': '#ff8a84',
    seed: '#7d8cff',
    scroll: '#2c2c2e',
    'scroll-hover': '#48484a'
  },
  light: {
    bg: '#e3e7ed',
    panel: '#edf0f4',
    raised: '#f6f7f9',
    hover: '#e0e5ec',
    subtle: '#e9edf2',
    line: '#d2d8e1',
    'card-line': '#dbe1e9',
    'row-line': '#e2e7ed',
    'row-alt': '#eff2f5',
    'btn-hover': '#cfd6e0',
    white: '#141a26',
    'on-primary': '#ffffff',
    menu: '#ffffff',
    'fg-soft': '#1f2430',
    'primary-hover': '#3a3a3c',
    muted: '#4a5568',
    dim: '#788498',
    ok: '#248a3d',
    warn: '#b86e00',
    bad: '#d70015',
    'bad-soft': '#d70015',
    seed: '#5856d6',
    scroll: '#c8c8ce',
    'scroll-hover': '#a8a8ae'
  }
};

const LEVELS = [
  { window: 0.7, panel: 0.4, raised: 0.6, tint: 0.06 },
  { window: 0.5, panel: 0.32, raised: 0.52, tint: 0.1 },
  { window: 0.3, panel: 0.26, raised: 0.46, tint: 0.16 }
];

type RGB = [number, number, number];

function rgb(hex: string): RGB {
  const h = hex.replace('#', '');
  return [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16)) as RGB;
}

function mix(a: RGB, b: RGB, amount: number): RGB {
  return a.map((v, i) => Math.round(v + (b[i] - v) * amount)) as RGB;
}

function luminance([r, g, b]: RGB): number {
  const ch = (v: number) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * ch(r) + 0.7152 * ch(g) + 0.0722 * ch(b);
}

function contrast(a: RGB, b: RGB): number {
  const [x, y] = [luminance(a), luminance(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
}

function readable(color: RGB, against: RGB, toward: RGB, min: number): RGB {
  let out = color;
  for (let i = 0; i < 16 && contrast(out, against) < min; i++) out = mix(out, toward, 0.1);
  return out;
}

function css([r, g, b]: RGB, alpha = 1): string {
  return alpha >= 1 ? `rgb(${r} ${g} ${b})` : `rgb(${r} ${g} ${b} / ${alpha})`;
}

const media = typeof window !== 'undefined' ? window.matchMedia('(prefers-color-scheme: dark)') : null;

export function resolveTheme(pref: string): ThemeName {
  if (pref === 'dark' || pref === 'light') return pref;
  return media?.matches === false ? 'light' : 'dark';
}

export function accentOf(id: string) {
  return ACCENTS.find((a) => a.id === id) ?? ACCENTS[0];
}

const GLOW_TARGETS = '.btn, .download-card, .sidebar-tile, .field, .bg-menu, .glow-target';
let glowTracking = false;

let spotTracking = false;

function trackSpotlight() {
  if (spotTracking || typeof window === 'undefined') return;
  spotTracking = true;
  let frame = 0;
  let x = 0;
  let y = 0;
  window.addEventListener(
    'pointermove',
    (e) => {
      x = e.clientX;
      y = e.clientY;
      if (frame) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        const root = document.documentElement;
        if (root.classList.contains('motion-off')) return;
        root.style.setProperty('--gx', `${Math.round((x / window.innerWidth) * 100)}%`);
        root.style.setProperty('--gy', `${Math.round((y / window.innerHeight) * 100)}%`);
      });
    },
    { passive: true }
  );
}

function trackGlow() {
  if (glowTracking || typeof window === 'undefined') return;
  glowTracking = true;
  let frame = 0;
  let last: PointerEvent | null = null;
  window.addEventListener(
    'pointermove',
    (e) => {
      last = e;
      if (frame) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        if (!last || !document.documentElement.classList.contains('glow')) return;
        const el = (last.target as Element | null)?.closest?.(GLOW_TARGETS) as HTMLElement | null;
        if (!el) return;
        const r = el.getBoundingClientRect();
        el.style.setProperty('--mx', `${last.clientX - r.left}px`);
        el.style.setProperty('--my', `${last.clientY - r.top}px`);
      });
    },
    { passive: true }
  );
}

const RIPPLE_SURFACES =
  '.btn, .icon-btn, .download-card, .sidebar-tile, .glow-target, .field, .chip, button, a, label, summary, select, input, textarea, [role="button"], [role="switch"], [role="tab"], .bg-menu';
let rippleTracking = false;

function inkInside(el: HTMLElement, x: number, y: number) {
  const r = el.getBoundingClientRect();
  if (r.width < 4 || r.height < 4) return false;
  const clip = document.createElement('span');
  clip.className = 'ripple-clip';
  clip.style.left = `${r.left}px`;
  clip.style.top = `${r.top}px`;
  clip.style.width = `${r.width}px`;
  clip.style.height = `${r.height}px`;
  clip.style.borderRadius = getComputedStyle(el).borderRadius || '8px';
  const dx = Math.max(x - r.left, r.right - x);
  const dy = Math.max(y - r.top, r.bottom - y);
  const size = Math.ceil(Math.hypot(dx, dy) * 2);
  const ink = document.createElement('span');
  ink.className = 'ripple-ink';
  ink.style.width = ink.style.height = `${size}px`;
  ink.style.left = `${x - r.left}px`;
  ink.style.top = `${y - r.top}px`;
  clip.appendChild(ink);
  document.body.appendChild(clip);
  ink.addEventListener('animationend', () => clip.remove(), { once: true });
  setTimeout(() => clip.remove(), 1500);
  return true;
}

function waveAt(x: number, y: number) {
  const wave = document.createElement('span');
  wave.className = 'ripple-wave';
  wave.style.left = `${x}px`;
  wave.style.top = `${y}px`;
  document.body.appendChild(wave);
  wave.addEventListener('animationend', () => wave.remove(), { once: true });
  setTimeout(() => wave.remove(), 1500);
}

function trackRipple() {
  if (rippleTracking || typeof window === 'undefined') return;
  rippleTracking = true;
  window.addEventListener(
    'pointerdown',
    (e) => {
      if (e.button !== 0 || !document.documentElement.classList.contains('ripple')) return;
      if (document.documentElement.classList.contains('motion-off')) return;
      const target = e.target as Element | null;
      if (!target) return;
      const surface = target.closest?.(RIPPLE_SURFACES) as HTMLElement | null;
      if (surface && surface.closest('.ripple-clip, .ripple-wave')) return;
      if (!surface || !inkInside(surface, e.clientX, e.clientY)) waveAt(e.clientX, e.clientY);
    },
    { passive: true, capture: true }
  );
}

const SHADED = new Set<string>([...SURFACES, 'line', 'card-line', 'row-line', 'row-alt', 'menu', 'btn-hover', 'scroll', 'scroll-hover']);

function brightened(palette: Palette, shade: number): Palette {
  const target: RGB = shade < 0 ? [0, 0, 0] : [255, 255, 255];
  const amount = (Math.abs(shade) / 100) * 0.8;
  const out = { ...palette };
  for (const [name, value] of Object.entries(palette)) {
    if (!SHADED.has(name)) continue;
    const [r, g, b] = mix(rgb(value), target, amount);
    out[name as keyof Palette] = '#' + [r, g, b].map((v) => v.toString(16).padStart(2, '0')).join('');
  }
  return out;
}

let applied = '';
let effectActive = false;
let lastAppearance: Appearance | null = null;

export async function applyAppearance(a: Appearance, windowEffect = true) {
  lastAppearance = a;
  const theme = resolveTheme(a.theme);
  const shade = Math.max(-30, Math.min(30, Math.round((theme === 'light' ? a.lightBrightness : a.darkBrightness) ?? 0)));
  const palette: Palette = shade === 0 ? PALETTES[theme] : brightened(PALETTES[theme], shade);
  const level = LEVELS[Math.min(Math.max(a.transparencyLevel, 0), 2)];
  const customHex = a.accentColor === 'custom' && /^#[0-9a-f]{6}$/i.test(a.accentCustom ?? '') ? a.accentCustom! : null;
  const accent = rgb(customHex ?? accentOf(a.accentColor).color);
  const black: RGB = [0, 0, 0];

  document.documentElement.classList.toggle('glass', a.glassEffect !== false);
  const glow = a.glassEffect !== false && a.hoverGlow !== false;
  document.documentElement.classList.toggle('glow', glow);
  if (glow) trackGlow();
  document.documentElement.classList.toggle('ripple', a.clickRipple !== false);
  document.documentElement.classList.toggle('tint', a.stateGlow === true);
  const reduced = typeof window !== 'undefined' && !!window.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
  document.documentElement.classList.toggle('motion-off', a.motion === 'off' || (a.motion === 'system' && reduced));
  for (const fx of ['aurora', 'spotlight', 'cards', 'progress', 'live', 'accents', 'transitions', 'state', 'fill']) {
    document.documentElement.classList.toggle(`no-fx-${fx}`, (a.effectsOff ?? []).includes(fx));
  }
  const heading = a.headingColor ?? 'accent';
  document.documentElement.dataset.heading = /^#[0-9a-f]{6}$/i.test(heading) ? 'custom' : heading;
  if (/^#[0-9a-f]{6}$/i.test(heading)) {
    const panel = rgb(PALETTES[resolveTheme(a.theme)].panel);
    const dark = resolveTheme(a.theme) === 'dark';
    const shown = a.accentContrast === 'exact' ? rgb(heading) : readable(rgb(heading), panel, dark ? [255, 255, 255] : [0, 0, 0], 4.5);
    document.documentElement.style.setProperty('--heading-custom', css(shown));
  }
  if (a.clickRipple !== false) trackRipple();
  trackSpotlight();
  document.documentElement.classList.toggle('rounded', a.roundedCorners !== false);
  document.documentElement.classList.toggle('wallpaper', !!a.wallpaper);
  const key = JSON.stringify([a.theme, theme, a.transparency, a.backdropMaterial, a.accentColor, customHex, level, shade]);
  if (!windowEffect) {
    effectActive = false;
  } else if (key !== applied) {
    applied = key;
    const tintBase = mix(rgb(palette.bg), accent, level.tint);
    try {
      effectActive = await api.applyAppearance(
        a.theme,
        a.transparency,
        a.backdropMaterial,
        [...tintBase, Math.round(255 * (theme === 'dark' ? 0.2 : 0.15))] as [number, number, number, number]
      );
    } catch {
      effectActive = false;
    }
  }

  const root = document.documentElement;
  root.dataset.theme = theme;
  root.dataset.transparent = String(effectActive);
  root.style.colorScheme = theme;
  const surfaceAlpha: Record<string, number> = {
    bg: 0,
    subtle: level.panel * 0.6,
    panel: level.panel,
    raised: level.raised,
    hover: level.raised
  };
  for (const [name, value] of Object.entries(palette)) {
    let color = rgb(value);
    let alpha = 1;
    const wp = a.wallpaper ? Math.min(Math.max((a.wallpaperOpacity ?? 72) / 100, 0.2), 0.98) : 0;
    if (wp && (SURFACES as readonly string[]).includes(name)) {
      const wallAlpha: Record<string, number> = { bg: wp * 0.55, subtle: wp * 0.7, panel: wp, raised: Math.min(1, wp + 0.1), hover: Math.min(1, wp + 0.1) };
      alpha = wallAlpha[name];
    } else if (effectActive && (SURFACES as readonly string[]).includes(name)) {
      color = mix(color, accent, level.tint);
      alpha = surfaceAlpha[name];
    } else if (theme === 'dark' && [...SURFACES, 'line', 'card-line', 'row-line', 'row-alt', 'menu', 'btn-hover'].includes(name)) {
      color = mix(color, accent, name === 'hover' || name === 'btn-hover' ? 0.09 : 0.035);
      if (name === 'bg' && a.motion !== 'off') alpha = 0.9;
    }
    root.style.setProperty(`--color-${name}`, css(color, alpha));
  }
  const windowColor = mix(rgb(palette.bg), accent, level.tint);
  root.style.setProperty('--color-window', css(windowColor, effectActive ? level.window : 1));
  const white: RGB = [255, 255, 255];
  const bgColor = rgb(palette.bg);
  let accentMain = theme === 'dark' ? accent : mix(accent, black, 0.12);
  if (a.accentContrast !== 'exact') accentMain = readable(accentMain, bgColor, theme === 'dark' ? white : black, 4.5);
  const accentStrong = mix(accentMain, black, theme === 'dark' ? 0.15 : 0.25);
  root.style.setProperty('--color-accent', css(accentMain));
  root.style.setProperty('--color-accent-strong', css(accentStrong));
  const customButton = /^#[0-9a-f]{6}$/i.test(a.buttonColor ?? '') ? rgb(a.buttonColor!) : null;
  let buttonTint = customButton ?? accentMain;
  if (customButton && a.accentContrast !== 'exact') buttonTint = readable(customButton, bgColor, theme === 'dark' ? white : black, 3);
  const buttonFill = customButton ? mix(buttonTint, black, theme === 'dark' ? 0.15 : 0.25) : accentStrong;
  root.style.setProperty('--btn-tint', css(buttonTint));
  root.style.setProperty('--btn-fill', css(buttonFill));
  root.style.setProperty('--btn-strength', customButton ? '22%' : '9%');
  const manualText = /^#[0-9a-f]{6}$/i.test(a.accentText ?? '') ? a.accentText! : null;
  if (manualText) root.style.setProperty('--btn-text', manualText);
  else root.style.removeProperty('--btn-text');
  const onAccent = manualText ?? (contrast(white, buttonFill) >= contrast(rgb('#0b0f14'), buttonFill) ? '#ffffff' : '#0b0f14');
  root.style.setProperty('--color-on-accent', onAccent);
}

media?.addEventListener('change', () => {
  if (lastAppearance && lastAppearance.theme === 'system') {
    applied = '';
    applyAppearance(lastAppearance);
  }
});

export function isTransparent() {
  return effectActive;
}
