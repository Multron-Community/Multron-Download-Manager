export const BUILTIN_PREFIX = 'builtin:';

export const BACKGROUND_AUTHOR = 'Karem Adem';

const PHOTOS = '-HDTIxZzgxI F9_fGS1pC2M VYhlOMPORj8 nCk7iiQCn8Y qPjTJttOByU'.split(' ');

export const BUILTIN_BACKGROUNDS = PHOTOS.map((photo, i) => {
  const id = `karem-adem-${i + 1}`;
  return {
    id,
    value: BUILTIN_PREFIX + id,
    image: `/backgrounds/${id}.jpg`,
    thumb: `/backgrounds/${id}-thumb.jpg`,
    author: BACKGROUND_AUTHOR,
    page: `https://unsplash.com/photos/${photo}`
  };
});

export function builtinBackground(value: string | null | undefined) {
  if (!value?.startsWith(BUILTIN_PREFIX)) return null;
  return BUILTIN_BACKGROUNDS.find((b) => b.value === value) ?? null;
}
