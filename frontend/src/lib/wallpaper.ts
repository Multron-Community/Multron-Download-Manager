export async function prepareWallpaper(src: string, blur: number): Promise<string> {
  const img = new Image();
  img.src = src;
  await img.decode();
  const nw = img.naturalWidth;
  const nh = img.naturalHeight;
  if (!nw || !nh) return src;
  const dpr = window.devicePixelRatio || 1;
  const width = blur > 0 ? Math.max(1, window.innerWidth) : Math.max(window.screen.width, window.innerWidth);
  const height = blur > 0 ? Math.max(1, window.innerHeight) : Math.max(window.screen.height, window.innerHeight);
  const cover = Math.max(width / nw, height / nh);
  const factor = Math.min(1, cover * dpr) * (blur >= 6 ? 0.5 : 1);
  if (blur <= 0 && factor >= 0.9) return src;
  const cw = Math.max(1, Math.round(nw * factor));
  const ch = Math.max(1, Math.round(nh * factor));
  const canvas = document.createElement('canvas');
  canvas.width = cw;
  canvas.height = ch;
  const ctx = canvas.getContext('2d');
  if (!ctx) return src;
  const radius = blur > 0 ? (blur / cover) * factor : 0;
  const margin = radius * 2;
  if (radius) ctx.filter = `blur(${radius.toFixed(2)}px)`;
  ctx.imageSmoothingQuality = 'high';
  ctx.drawImage(img, -margin, -margin, cw + margin * 2, ch + margin * 2);
  const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, 'image/webp', 0.9));
  canvas.width = canvas.height = 0;
  return blob ? URL.createObjectURL(blob) : src;
}
