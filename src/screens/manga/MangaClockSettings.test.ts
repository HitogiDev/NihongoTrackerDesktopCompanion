import { describe, expect, it } from 'vitest';
import { boundedNumber } from './MangaClockSettings';

describe('clock settings', () => {
  it('reads a whole number and brings it into range', () => {
    expect(boundedNumber('5', 0, 600)).toBe(5);
    expect(boundedNumber('2.6', 0, 600)).toBe(3);
    expect(boundedNumber('-4', 0, 600)).toBe(0);
    expect(boundedNumber('9999', 1, 120)).toBe(120);
  });

  it('gives nothing for an empty or broken field, so the saved value stays', () => {
    expect(boundedNumber('', 0, 600)).toBeNull();
    expect(boundedNumber('  ', 0, 600)).toBeNull();
    expect(boundedNumber('abc', 0, 600)).toBeNull();
  });
});
