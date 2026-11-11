import type { LogPayload, OutboxItem } from './types';

/**
 * The send queue as the Queue screen shows it: what still has to leave (queued
 * or failed, newest first as the backend lists them) and the last few sent.
 */
export function splitOutbox(items: OutboxItem[], sentLimit = 15) {
  return {
    waiting: items.filter((item) => item.status !== 'sent'),
    sent: items.filter((item) => item.status === 'sent').slice(0, sentLimit),
  };
}

/** Only logs can be edited: line pushes and timers are rebuilt from local state. */
export function isEditableLog(item: OutboxItem): item is OutboxItem & { payload: LogPayload } {
  return item.kind === 'log' && item.status !== 'sent';
}

/** What a queue row is called: the work, then the comment, then its kind. */
export function outboxLabel(item: OutboxItem): string {
  const payload = item.payload as Partial<LogPayload>;
  return payload.mediaTitle || payload.description || item.kind;
}

/** " (#1–3)" for a run, " (#1, 3)" otherwise, " (#2)" for one episode. */
export function episodeLabel(episodes: number[] | undefined, single: number | null | undefined): string {
  const list = episodes?.length ? [...episodes].sort((a, b) => a - b) : single ? [single] : [];
  if (list.length === 0) return '';
  if (list.length === 1) return ` (#${list[0]})`;
  const consecutive = list.every((n, i) => i === 0 || n === list[i - 1] + 1);
  return consecutive ? ` (#${list[0]}–${list[list.length - 1]})` : ` (#${list.join(', ')})`;
}
