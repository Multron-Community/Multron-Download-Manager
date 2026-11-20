import { invoke } from '@tauri-apps/api/core';

export type LanguageInfo = { code: string; name: string; english?: string; strings?: number };

export const LANGUAGES: LanguageInfo[] = $state([{ code: 'en', name: 'English' }]);

let listed: Promise<void> | null = null;

function setList(list: LanguageInfo[] | null | undefined) {
  if (!Array.isArray(list) || !list.length) return;
  LANGUAGES.splice(0, LANGUAGES.length, ...list);
  if (!LANGUAGES.some((l) => l.code === 'en')) LANGUAGES.unshift({ code: 'en', name: 'English' });
}

function ensureList(): Promise<void> {
  if (!listed) {
    listed = invoke<LanguageInfo[]>('language_list')
      .then(setList)
      .catch(() => {});
  }
  return listed;
}

const known = (code: string) => LANGUAGES.some((l) => l.code === code);

export function systemLanguage(): string {
  const tags = (navigator.languages?.length ? navigator.languages : [navigator.language]).map((l) => l.toLowerCase());
  for (const tag of tags) {
    const candidates = [tag];
    if (tag.startsWith('zh')) candidates.push(/hant|-tw|-hk|-mo/.test(tag) ? 'zh-tw' : 'zh-cn');
    if (tag.startsWith('pt')) candidates.push('pt-br');
    candidates.push(tag.split('-')[0]);
    const hit = candidates.find(known);
    if (hit) return hit;
  }
  return 'en';
}

export const i18n = $state({ setting: 'en', lang: 'en' });

let dictionary = $state.raw<Record<string, string>>({});
const cache = new Map<string, Record<string, string>>();
let request = 0;

async function load(code: string): Promise<Record<string, string>> {
  if (!known(code)) return {};
  const hit = cache.get(code);
  if (hit) return hit;
  const dict = (await invoke<Record<string, string>>('language_strings', { code })) || {};
  cache.set(code, dict);
  return dict;
}

export async function setLanguage(setting: string) {
  const ticket = ++request;
  await ensureList();
  const lang = setting === 'auto' ? systemLanguage() : known(setting) ? setting : 'en';
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

export async function reloadLanguages() {
  cache.clear();
  listed = invoke<LanguageInfo[]>('language_reload')
    .then(setList)
    .catch(() => {});
  await listed;
  await setLanguage(i18n.setting);
}

export async function openLanguageFolder(): Promise<string> {
  return invoke<string>('language_folder');
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
