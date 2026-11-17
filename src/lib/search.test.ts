import { describe, expect, it } from 'vitest';
import { matchesSearch, normalizeSearch } from './search';

describe('search', () => {
  it('ignores case and accents', () => {
    expect(normalizeSearch('Detección')).toBe('deteccion');
    expect(matchesSearch('Detección de reproductores', 'DETECCION')).toBe(true);
  });

  it('needs every word, in any order', () => {
    expect(matchesSearch('Open texthooker', 'texthooker open')).toBe(true);
    expect(matchesSearch('Open texthooker', 'open library')).toBe(false);
    expect(matchesSearch('Anything', '   ')).toBe(true);
  });
});
