import type { LogPayload } from './types';

export const LOG_TYPES = [
  'anime',
  'manga',
  'reading',
  'vn',
  'video',
  'movie',
  'tv show',
  'audio',
  'game',
  'book',
  'other',
] as const;

export type Amount = 'episodes' | 'time' | 'chars' | 'pages';

/**
 * What each field means and which ones a type accepts, mirroring
 * `Backend/src/models/log.model.ts`. Checked here so a log never reaches the
 * outbox only to bounce off the server's validation.
 *
 * `all` must be filled in; `any` needs at least one of them.
 */
export const RULES: Record<string, { fields: Amount[]; all?: Amount[]; any?: Amount[] }> = {
  anime: { fields: ['episodes', 'time'], all: ['episodes'] },
  manga: { fields: ['pages', 'chars', 'time'], any: ['pages', 'chars', 'time'] },
  reading: { fields: ['pages', 'chars', 'time'], any: ['pages', 'chars', 'time'] },
  book: { fields: ['pages', 'chars', 'time'], any: ['pages', 'chars', 'time'] },
  vn: { fields: ['chars', 'time'], any: ['chars', 'time'] },
  game: { fields: ['chars', 'time'], any: ['chars', 'time'] },
  video: { fields: ['time'], all: ['time'] },
  movie: { fields: ['time'], all: ['time'] },
  audio: { fields: ['time'], all: ['time'] },
  other: { fields: ['time'], all: ['time'] },
  'tv show': { fields: ['episodes', 'time'] },
};

/** Types `/api/media/search` knows about; the rest fall back to anime. */
export const SEARCHABLE = ['anime', 'vn', 'manga', 'reading', 'game', 'movie', 'tv show', 'book'];

export function ruleFor(type: string) {
  return RULES[type] ?? { fields: [] as Amount[] };
}

/** A form value as a positive amount, or `undefined` when it isn't one. */
export function amount(value: string | number | undefined | null): number | undefined {
  if (value === undefined || value === null) return undefined;
  const text = String(value).trim();
  const parsed = Number(text);
  return text && Number.isFinite(parsed) && parsed > 0 ? parsed : undefined;
}

/** The editable side of a log, every amount kept as typed. */
export interface LogDraft {
  type: string;
  mediaTitle: string;
  comment: string;
  mediaId: string;
  image: string | null;
  episodes: string;
  time: string;
  chars: string;
  pages: string;
  /** `datetime-local` value, empty for "now". */
  date: string;
}

export const EMPTY_DRAFT: LogDraft = {
  type: 'anime',
  mediaTitle: '',
  comment: '',
  mediaId: '',
  image: null,
  episodes: '',
  time: '',
  chars: '',
  pages: '',
  date: '',
};

/** `YYYY-MM-DDTHH:mm` in local time, what `<input type="datetime-local">` takes. */
export function toLocalInput(iso: string | undefined): string {
  if (!iso) return '';
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return '';
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

/**
 * A queued payload as a draft. Detected logs keep the title in `description`
 * when they have no `mediaTitle`; it moves to the title so the comment starts
 * empty.
 */
export function draftFromPayload(payload: LogPayload): LogDraft {
  const titled = Boolean(payload.mediaTitle);
  const text = (value: number | undefined) => (value === undefined || value === null ? '' : String(value));
  return {
    type: payload.type || 'other',
    mediaTitle: (titled ? payload.mediaTitle : payload.description) ?? '',
    comment: titled ? payload.description ?? '' : '',
    mediaId: payload.mediaId ?? '',
    image: payload._detected?.image ?? null,
    episodes: text(payload.episodes),
    time: text(payload.time),
    chars: text(payload.chars),
    pages: text(payload.pages),
    date: toLocalInput(payload.date),
  };
}

/**
 * The body for `POST /api/logs`. Amounts the type doesn't take are dropped, so
 * switching a draft from anime to vn can't send stray episodes. `description`
 * is always present: the backend requires it even though its schema says
 * optional, and an unlinked log uses the title.
 */
export function toPayload(draft: LogDraft, now = new Date()): LogPayload {
  const title = draft.mediaTitle.trim();
  const mediaId = draft.mediaId.trim();
  const date = draft.date ? new Date(draft.date) : now;
  const payload: LogPayload = {
    type: draft.type,
    mediaTitle: title,
    description: draft.comment.trim() || (mediaId ? '' : title),
    date: (Number.isNaN(date.getTime()) ? now : date).toISOString(),
  };
  if (mediaId) payload.mediaId = mediaId;
  for (const field of ruleFor(draft.type).fields) {
    const value = amount(draft[field]);
    if (value !== undefined) payload[field] = value;
  }
  return payload;
}

/** `null` when the draft is ready to send, otherwise the key of what's missing. */
export function missing(draft: LogDraft): Amount | 'mediaTitle' | 'any' | null {
  if (!draft.mediaTitle.trim()) return 'mediaTitle';
  const rule = ruleFor(draft.type);
  for (const field of rule.all ?? []) {
    if (amount(draft[field]) === undefined) return field;
  }
  if (rule.any && !rule.any.some((field) => amount(draft[field]) !== undefined)) {
    return 'any';
  }
  return null;
}
