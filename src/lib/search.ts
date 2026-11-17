/** Lower case and without accents, so "deteccion" finds "Detección". */
export function normalizeSearch(text: string): string {
  return text.normalize('NFD').replace(/\p{Diacritic}/gu, '').toLowerCase();
}

/** Every word of `query` appears somewhere in `text`, in any order. */
export function matchesSearch(text: string, query: string): boolean {
  const haystack = normalizeSearch(text);
  return normalizeSearch(query).split(/\s+/).filter(Boolean).every((word) => haystack.includes(word));
}
