export function tagHue(name: string) {
  let h = 0;
  for (const ch of name.toLowerCase()) h = (h * 31 + ch.charCodeAt(0)) % 360;
  return h;
}

export function tagStyle(name: string) {
  const h = tagHue(name);
  return `background: hsl(${h} 70% 50% / 0.16); color: hsl(${h} 80% 68%); border-color: hsl(${h} 70% 55% / 0.35);`;
}

export function signatureView(sig: string | undefined) {
  if (!sig) return null;
  if (sig.startsWith('valid:')) return { tone: 'ok' as const, text: sig.slice(6) || 'Signed', valid: true };
  if (sig === 'unsigned') return { tone: 'warn' as const, text: 'Not signed', valid: false };
  return { tone: 'bad' as const, text: 'Signature invalid', valid: false };
}
