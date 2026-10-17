const loaders = import.meta.glob<Record<string, string>>('../lang/*.json', { import: 'default' });

const files: Record<string, () => Promise<Record<string, string>>> = Object.fromEntries(
  Object.entries(loaders).map(([file, load]) => [file.replace(/^.*\/|\.json$/g, ''), load])
);

const NAMES: [string, string][] = [
  ['en', 'English'],
  ['tr', 'Türkçe'],
  ['de', 'Deutsch'],
  ['fr', 'Français'],
  ['es', 'Español'],
  ['it', 'Italiano'],
  ['pt-br', 'Português (Brasil)'],
  ['ru', 'Русский'],
  ['ja', '日本語'],
  ['zh-cn', '简体中文'],
  ['zh-tw', '繁體中文'],
  ['ko', '한국어']
];

export const LANGUAGES = NAMES.filter(([code]) => code === 'en' || code in files).map(([code, name]) => ({
  code,
  name
}));

export function systemLanguage(): string {
  const tags = (navigator.languages?.length ? navigator.languages : [navigator.language]).map((l) => l.toLowerCase());
  for (const tag of tags) {
    const candidates = [tag];
    if (tag.startsWith('zh')) candidates.push(/hant|-tw|-hk|-mo/.test(tag) ? 'zh-tw' : 'zh-cn');
    if (tag.startsWith('pt')) candidates.push('pt-br');
    candidates.push(tag.split('-')[0]);
    const hit = candidates.find((c) => c === 'en' || c in files);
    if (hit) return hit;
  }
  return 'en';
}

export const i18n = $state({ setting: 'en', lang: 'en' });

let dictionary = $state.raw<Record<string, string>>({});
const cache = new Map<string, Record<string, string>>();
let request = 0;

async function load(code: string): Promise<Record<string, string>> {
  if (code === 'en' || !(code in files)) return {};
  const hit = cache.get(code);
  if (hit) return hit;
  const dict = await files[code]();
  cache.set(code, dict);
  return dict;
}

export async function setLanguage(setting: string) {
  const lang = setting === 'auto' ? systemLanguage() : setting;
  const ticket = ++request;
  let dict: Record<string, string> = {};
  try {
    dict = await load(lang);
  } catch {
    dict = {};
  }
  if (ticket !== request) return;
  i18n.setting = setting;
  i18n.lang = lang;
  dictionary = dict;
  document.documentElement.lang = lang;
}

export function t(key: string, vars?: Record<string, string | number>): string {
  return fill(dictionary[key] || key, vars);
}

export function tc(context: string, key: string, vars?: Record<string, string | number>): string {
  return fill(dictionary[`${context}|${key}`] || key, vars);
}

function fill(s: string, vars?: Record<string, string | number>): string {
  if (vars) {
    for (const [k, v] of Object.entries(vars)) s = s.replaceAll(`{${k}}`, String(v));
  }
  return s;
}
