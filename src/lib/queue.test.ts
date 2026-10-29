import { describe, expect, it } from 'vitest';
import { isEditableLog, outboxLabel, splitOutbox } from './queue';
import type { OutboxItem } from './types';

const item = (id: string, patch: Partial<OutboxItem> = {}): OutboxItem => ({
  id,
  kind: 'log',
  status: 'queued',
  attempts: 0,
  lastError: null,
  createdAt: '2026-10-28T12:00:00Z',
  payload: { type: 'anime', description: '', mediaTitle: id },
  ...patch,
});

describe('splitOutbox', () => {
  it('keeps queued and failed apart from sent, in order', () => {
    const items = [item('a'), item('b', { status: 'sent' }), item('c', { status: 'failed' })];
    const { waiting, sent } = splitOutbox(items);
    expect(waiting.map((i) => i.id)).toEqual(['a', 'c']);
    expect(sent.map((i) => i.id)).toEqual(['b']);
  });

  it('caps the sent list', () => {
    const items = Array.from({ length: 20 }, (_, n) => item(String(n), { status: 'sent' }));
    expect(splitOutbox(items).sent).toHaveLength(15);
    expect(splitOutbox(items, 3).sent.map((i) => i.id)).toEqual(['0', '1', '2']);
  });
});

describe('isEditableLog', () => {
  it('only lets unsent logs be edited', () => {
    expect(isEditableLog(item('a'))).toBe(true);
    expect(isEditableLog(item('a', { status: 'failed' }))).toBe(true);
    expect(isEditableLog(item('a', { status: 'sent' }))).toBe(false);
    expect(isEditableLog(item('a', { kind: 'texthooker_lines', payload: {} }))).toBe(false);
  });
});

describe('outboxLabel', () => {
  it('falls back from title to description to kind', () => {
    expect(outboxLabel(item('a'))).toBe('a');
    expect(outboxLabel(item('a', { payload: { type: 'vn', description: 'AIR' } }))).toBe('AIR');
    expect(outboxLabel(item('a', { kind: 'texthooker_timer', payload: {} }))).toBe('texthooker_timer');
  });
});
