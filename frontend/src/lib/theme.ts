
import { api } from './api';

export type ThemePref = 'system' | 'dark' | 'light';
export type ThemeName = 'dark' | 'light';

export interface Appearance {
  theme: string;
  accentColor: string;
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
    bg: '#161617',
    panel: '#1e1e1f',
    raised: '#232325',
    hover: '#252528',
    subtle: '#1a1a1c',
    line: '#2c2c2e',
    'card-line': '#222223',
    'row-line': '#222224',
    'row-alt': '#19191a',
    'btn-hover': '#3a3a3d',
    white: '#ffffff',
    'on-primary': '#161617',
    menu: '#232325',
    'fg-soft': '#e5e5ea',
    'primary-hover': '#e2e2e6',
    muted: '#8e8e93',
    dim: '#555558',
    ok: '#34c759',
    warn: '#ffb340',
    bad: '#ff5a52',
    'bad-soft': '#ff8a84',
    seed: '#7d8cff',
    scroll: '#2c2c2e',
    'scroll-hover': '#48484a'
  },
  light: {
    bg: '#f3f3f5',
    panel: '#fafafb',
    raised: '#ffffff',
    hover: '#ebebef',
    subtle: '#f7f7f9',
    line: '#e0e0e5',
    'card-line': '#e6e6ea',
    'row-line': '#ececf0',
    'row-alt': '#f6f6f8',
    'btn-hover': '#d5d5db',
    white: '#1c1c1e',
    'on-primary': '#ffffff',
    menu: '#ffffff',
    'fg-soft': '#2c2c2e',
    'primary-hover': '#3a3a3c',
    muted: '#6e6e73',
    dim: '#a1a1a6',
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

let applied = '';
let effectActive = false;
let lastAppearance: Appearance | null = null;

export async function applyAppearance(a: Appearance, windowEffect = true) {
  lastAppearance = a;
  const theme = resolveTheme(a.theme);
  const palette = PALETTES[theme];
  const level = LEVELS[Math.min(Math.max(a.transparencyLevel, 0), 2)];
  const accent = rgb(accentOf(a.accentColor).color);
  const black: RGB = [0, 0, 0];

  const key = JSON.stringify([a.theme, theme, a.transparency, a.backdropMaterial, a.accentColor, level]);
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
    if (effectActive && (SURFACES as readonly string[]).includes(name)) {
      color = mix(color, accent, level.tint);
      alpha = surfaceAlpha[name];
    }
    root.style.setProperty(`--color-${name}`, css(color, alpha));
  }
  const windowColor = mix(rgb(palette.bg), accent, level.tint);
  root.style.setProperty('--color-window', css(windowColor, effectActive ? level.window : 1));
  const accentMain = theme === 'dark' ? accent : mix(accent, black, 0.12);
  const accentStrong = mix(accent, black, theme === 'dark' ? 0.15 : 0.25);
  root.style.setProperty('--color-accent', css(accentMain));
  root.style.setProperty('--color-accent-strong', css(accentStrong));
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
