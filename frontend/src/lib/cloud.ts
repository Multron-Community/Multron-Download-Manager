import { t } from './i18n.svelte';

export type CloudTone = 'ok' | 'warn' | 'bad' | 'muted';

export interface CloudView {
  tone: CloudTone;
  icon: 'shield' | 'alert' | 'lock' | 'info';
  label: string;
  badge: string;
  reason: string;
}

function why(part: string): string {
  const big = part.match(/^(\d+) larger than the (\d+) MB upload limit$/);
  if (big) return t('{count} larger than the {limit} MB upload limit', { count: big[1], limit: big[2] });
  return part;
}

export function cloudReason(detail: string | null | undefined): string {
  let text = (detail ?? '').replace(/^Multron Cloud:?\s*/, '').trim();
  let stopped = '';
  const stop = text.match(/^(.*?)\s*\(stopped after (\d+) of (\d+) files\)$/);
  if (stop) {
    text = stop[1];
    stopped = t('Stopped after {done} of {total} files', { done: stop[2], total: stop[3] });
  }
  let m: RegExpMatchArray | null;
  if ((m = text.match(/^no threat found; (\d+) of (\d+) files are not verified as clean yet$/))) {
    text = t('No threat was found, but {count} of {total} files are not verified as clean yet.', { count: m[1], total: m[2] });
  } else if ((m = text.match(/^no threat found in (\d+) of (\d+) files; (\d+) could not be checked \((.*)\)$/))) {
    const reasons = m[4].split(', ').map(why).join(', ');
    text = t('No threat found in {checked} of {total} files. {missed} could not be checked: {why}.', { checked: m[1], total: m[2], missed: m[3], why: reasons });
  } else if ((m = text.match(/^the file is larger than the (\d+) MB upload limit and the cloud does not know its fingerprint$/))) {
    text = t('The file is larger than the {limit} MB upload limit and the cloud does not know its fingerprint, so it was not uploaded.', { limit: m[1] });
  } else if ((m = text.match(/^(\d+) of (\d+) files are larger than the (\d+) MB upload limit$/))) {
    text = t('{count} of {total} files are larger than the {limit} MB upload limit and the cloud does not know their fingerprints, so they were not uploaded.', { count: m[1], total: m[2], limit: m[3] });
  } else if ((m = text.match(/^larger than the (\d+) MB upload limit$/))) {
    text = t('Larger than the {limit} MB upload limit', { limit: m[1] });
  } else if (text === 'no file was checked') {
    text = t('No file was checked.');
  } else {
    text = text
      .replaceAll('password-protected archive; enter its password to scan the files inside', t('password-protected archive; enter its password to scan the files inside'))
      .replaceAll('password-protected RAR archive; extract it with WinRAR or 7-Zip and scan the files', t('password-protected RAR archive; extract it with WinRAR or 7-Zip and scan the files'));
  }
  return [text, stopped].filter(Boolean).join(' · ');
}

export function cloudView(state: string, detail?: string | null): CloudView {
  const reason = state === 'clean' ? '' : cloudReason(detail);
  switch (state) {
    case 'clean':
      return { tone: 'ok', icon: 'shield', label: t('Clean'), badge: t('Multron Cloud: clean'), reason };
    case 'unknown':
      return { tone: 'ok', icon: 'shield', label: t('No threat found'), badge: t('Multron Cloud: no threat found'), reason };
    case 'partial':
      return { tone: 'warn', icon: 'alert', label: t('Partly scanned'), badge: t('Multron Cloud: partly scanned'), reason };
    case 'threat':
      return { tone: 'bad', icon: 'alert', label: t('Threat found'), badge: t('Multron Cloud: threat'), reason };
    case 'suspicious':
      return { tone: 'warn', icon: 'alert', label: t('Suspicious file'), badge: t('Multron Cloud: suspicious'), reason };
    case 'locked':
      return { tone: 'warn', icon: 'lock', label: t('Password needed'), badge: t('Multron Cloud: password needed'), reason };
    case 'error':
      return { tone: 'warn', icon: 'alert', label: t('Scan not finished'), badge: t('Multron Cloud: scan not finished'), reason };
    case 'skipped':
      return { tone: 'warn', icon: 'alert', label: t('Not scanned'), badge: t('Multron Cloud: not scanned'), reason };
    default:
      return { tone: 'muted', icon: 'info', label: t('Not scanned'), badge: t('Multron Cloud: not scanned'), reason };
  }
}
