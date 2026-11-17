import { describe, expect, it } from 'vitest';
import { lineWindowStart, trimmedStart } from './lineWindow';

const lines = (count: number) => Array.from({ length: count }, (_, index) => ({ id: `l${index}` }));

describe('lineWindowStart', () => {
  it('shows the last lines when nothing is pinned', () => {
    expect(lineWindowStart(lines(1000), null, null, 300)).toBe(700);
    expect(lineWindowStart(lines(10), null, null, 300)).toBe(0);
    expect(lineWindowStart([], null, null, 300)).toBe(0);
  });

  it('stays on the pinned line when lines arrive below it', () => {
    expect(lineWindowStart(lines(1001), 'l700', 700, 300)).toBe(700);
  });

  it('keeps the index when the pinned line was deleted', () => {
    const remaining = lines(1000).filter((line) => line.id !== 'l700');
    expect(lineWindowStart(remaining, 'l700', 700, 300)).toBe(700);
    expect(lineWindowStart([], 'l700', 700, 300)).toBe(0);
  });

  it('falls back to the last lines without a previous index', () => {
    expect(lineWindowStart(lines(1000), 'gone', null, 300)).toBe(700);
  });
});

describe('trimmedStart', () => {
  it('trims only once the window grew past the slack', () => {
    expect(trimmedStart(1100, 700, 300, 100)).toBe(700);
    expect(trimmedStart(1101, 700, 300, 100)).toBe(801);
  });
});
