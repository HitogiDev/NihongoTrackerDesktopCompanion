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
