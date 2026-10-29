import { describe, expect, it } from 'vitest';
import { EMPTY_DRAFT, draftFromPayload, missing, toPayload, type LogDraft } from './logRules';

const draft = (patch: Partial<LogDraft>): LogDraft => ({ ...EMPTY_DRAFT, ...patch });

describe('missing', () => {
  it('asks for a title before anything else', () => {
    expect(missing(draft({ type: 'anime', episodes: '1' }))).toBe('mediaTitle');
  });

  it('requires what the backend requires per type', () => {
    expect(missing(draft({ type: 'anime', mediaTitle: 'x', time: '24' }))).toBe('episodes');
    expect(missing(draft({ type: 'anime', mediaTitle: 'x', episodes: '1' }))).toBeNull();
    expect(missing(draft({ type: 'vn', mediaTitle: 'x' }))).toBe('any');
    expect(missing(draft({ type: 'vn', mediaTitle: 'x', chars: '1200' }))).toBeNull();
    expect(missing(draft({ type: 'video', mediaTitle: 'x' }))).toBe('time');
  });

  it('does not count zero, negatives or junk as an amount', () => {
    expect(missing(draft({ type: 'video', mediaTitle: 'x', time: '0' }))).toBe('time');
    expect(missing(draft({ type: 'video', mediaTitle: 'x', time: '-5' }))).toBe('time');
    expect(missing(draft({ type: 'video', mediaTitle: 'x', time: 'abc' }))).toBe('time');
  });
});

describe('toPayload', () => {
  const now = new Date('2026-10-28T12:00:00Z');

  it('drops amounts the type does not take', () => {
    const payload = toPayload(draft({ type: 'vn', mediaTitle: 'AIR', episodes: '3', chars: '5000' }), now);
    expect(payload.episodes).toBeUndefined();
    expect(payload.chars).toBe(5000);
  });

  it('always sends a description, the title when unlinked', () => {
    expect(toPayload(draft({ type: 'video', mediaTitle: ' Vlog ', time: '10' }), now).description).toBe('Vlog');
    expect(toPayload(draft({ type: 'anime', mediaTitle: 'Show', mediaId: '1', episodes: '1' }), now).description).toBe('');
    expect(toPayload(draft({ type: 'anime', mediaTitle: 'Show', comment: ' fun ', episodes: '1' }), now).description).toBe('fun');
  });

  it('uses the chosen date, or now', () => {
    expect(toPayload(draft({ type: 'video', mediaTitle: 'x', time: '1' }), now).date).toBe(now.toISOString());
    const dated = toPayload(draft({ type: 'video', mediaTitle: 'x', time: '1', date: '2026-01-02T03:04' }), now);
    expect(dated.date).toBe(new Date('2026-01-02T03:04').toISOString());
  });
});

describe('draftFromPayload', () => {
  it('moves a detected title out of the description', () => {
    const detected = draftFromPayload({ type: 'vn', mediaId: 'v36', description: 'AIR', chars: 1200, time: 30 });
    expect(detected.mediaTitle).toBe('AIR');
    expect(detected.comment).toBe('');
    expect(detected.chars).toBe('1200');
  });

  it('round-trips a log without losing fields', () => {
    const payload = {
      type: 'manga',
      mediaTitle: 'Yotsuba',
      mediaId: '30104',
      description: 'vol 1',
      pages: 40,
      time: 35,
      date: '2026-10-20T10:30:00.000Z',
    };
    const back = toPayload(draftFromPayload(payload), new Date());
    // datetime-local has minute precision, which this date already has.
    expect(back).toEqual(payload);
  });
});
