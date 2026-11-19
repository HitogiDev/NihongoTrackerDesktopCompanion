import type { MangaSeries } from '../../lib/types';

/**
 * How the library grid is sorted and filtered. Pure, so the Manga screen can
 * run it on every keystroke over a thousand series: one pass to filter, one
 * sort, nothing asked of Rust.
 */
export const SORTS = ['title-asc', 'title-desc', 'last-read', 'added', 'volumes'] as const;
export type SortKey = (typeof SORTS)[number];

export const TEXT_FILTERS = ['any', 'with', 'without'] as const;
export const LINK_FILTERS = ['any', 'linked', 'unlinked', 'suggested'] as const;
export const STATUS_FILTERS = ['any', 'reading', 'finished', 'unstarted'] as const;
/** Where a series comes from: `MangaSeries.source`. */
export const ORIGIN_FILTERS = ['any', 'local', 'webdav', 'suwayomi'] as const;

export interface LibraryView {
  sort: SortKey;
  text: (typeof TEXT_FILTERS)[number];
  link: (typeof LINK_FILTERS)[number];
  status: (typeof STATUS_FILTERS)[number];
  origin: (typeof ORIGIN_FILTERS)[number];
}

export const DEFAULT_VIEW: LibraryView = { sort: 'title-asc', text: 'any', link: 'any', status: 'any', origin: 'any' };

/** The two sections of the Manga screen, each with its own view. */
export type Section = 'library' | 'all';

/**
 * "My library" opens on what was read last; "All series" is a catalogue,
 * in title order.
 */
export const SECTION_DEFAULTS: Record<Section, LibraryView> = {
  library: { ...DEFAULT_VIEW, sort: 'last-read' },
  all: DEFAULT_VIEW,
};

/** The view with its filters cleared, its order kept. */
export const cleared = (view: LibraryView): LibraryView => ({ ...DEFAULT_VIEW, sort: view.sort });

/** Titles the way a person numbers them (`2 < 10`), case and accents aside. */
const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' });
const naturalCompare = (a: string, b: string) => collator.compare(a, b);

export type SeriesStatus = 'reading' | 'finished' | 'unstarted';

/** Read in full, never opened, or somewhere in between. */
export function seriesStatus(series: MangaSeries): SeriesStatus {
  const volumes = Number(series.volume_count);
  if (volumes > 0 && Number(series.volumes_finished) >= volumes) return 'finished';
  if (Number(series.pages_read) === 0 && Number(series.volumes_finished) === 0 && !series.last_read_at) return 'unstarted';
  return 'reading';
}

/** How many filters (not the order) narrow the grid now. */
export function activeFilters(view: LibraryView): number {
  return Number(view.text !== 'any') + Number(view.link !== 'any') + Number(view.status !== 'any') + Number(view.origin !== 'any');
}

/** Whether one series passes the view's filters. */
export function matches(item: MangaSeries, view: LibraryView, suggested: ReadonlySet<string>): boolean {
  if (view.text === 'with' && !item.has_text) return false;
  if (view.text === 'without' && item.has_text) return false;
  if (view.link === 'linked' && !item.media_id) return false;
  if (view.link === 'unlinked' && item.media_id) return false;
  if (view.link === 'suggested' && (item.media_id || !suggested.has(item.id))) return false;
  if (view.status !== 'any' && seriesStatus(item) !== view.status) return false;
  if (view.origin !== 'any' && item.source !== view.origin) return false;
  return true;
}

export type FilterKey = 'text' | 'link' | 'status' | 'origin';

/**
 * For each choice of one filter, how many series it would leave with the
 * other filters as they are: the numbers next to each option.
 */
export function optionCounts<K extends FilterKey>(series: MangaSeries[], view: LibraryView, key: K, options: readonly LibraryView[K][], suggested: ReadonlySet<string>): Record<string, number> {
  const counts: Record<string, number> = {};
  for (const option of options) {
    const probe = { ...view, [key]: option };
    let count = 0;
    for (const item of series) if (matches(item, probe, suggested)) count += 1;
    counts[option] = count;
  }
  return counts;
}

/**
 * The series to show, in order. `suggested` holds the ids of series with a
 * link suggestion waiting for the user. The library comes from Rust in
 * natural title order; ties keep it.
 */
export function applyView(series: MangaSeries[], view: LibraryView, suggested: ReadonlySet<string> = new Set()): MangaSeries[] {
  const kept = series.filter((item) => matches(item, view, suggested));
  const byTitle = (a: MangaSeries, b: MangaSeries) => naturalCompare(a.title, b.title);
  switch (view.sort) {
    case 'title-asc':
      return kept.sort(byTitle);
    case 'title-desc':
      return kept.sort((a, b) => byTitle(b, a));
    case 'last-read':
      // Never read: after everything read, in title order.
      return kept.sort((a, b) => {
        if (a.last_read_at && b.last_read_at) return b.last_read_at.localeCompare(a.last_read_at) || byTitle(a, b);
        if (a.last_read_at) return -1;
        if (b.last_read_at) return 1;
        return byTitle(a, b);
      });
    case 'added':
      return kept.sort((a, b) => b.created_at.localeCompare(a.created_at) || byTitle(a, b));
    case 'volumes':
      return kept.sort((a, b) => Number(b.volume_count) - Number(a.volume_count) || byTitle(a, b));
  }
}

const VIEW_KEYS: Record<Section, string> = { library: 'manga:library:view', all: 'manga:all:view' };

function pick<T extends string>(value: unknown, allowed: readonly T[], fallback: T): T {
  return typeof value === 'string' && (allowed as readonly string[]).includes(value) ? (value as T) : fallback;
}

/** The last view the user chose for a section (private storage: the default). */
export function loadView(section: Section): LibraryView {
  const fallback = SECTION_DEFAULTS[section];
  try {
    const raw = JSON.parse(localStorage.getItem(VIEW_KEYS[section]) ?? 'null') as Partial<Record<keyof LibraryView, unknown>> | null;
    if (!raw || typeof raw !== 'object') return fallback;
    return {
      sort: pick(raw.sort, SORTS, fallback.sort),
      text: pick(raw.text, TEXT_FILTERS, 'any'),
      link: pick(raw.link, LINK_FILTERS, 'any'),
      status: pick(raw.status, STATUS_FILTERS, 'any'),
      origin: pick(raw.origin, ORIGIN_FILTERS, 'any'),
    };
  } catch {
    return fallback;
  }
}

export function saveView(section: Section, view: LibraryView) {
  try {
    localStorage.setItem(VIEW_KEYS[section], JSON.stringify(view));
  } catch {
    // Private storage: the view lasts until the window closes.
  }
}

const SECTION_KEY = 'manga:section';

/** The section the screen opened on last time. */
export function loadSection(): Section {
  try {
    return localStorage.getItem(SECTION_KEY) === 'all' ? 'all' : 'library';
  } catch {
    return 'library';
  }
}

export function saveSection(section: Section) {
  try {
    localStorage.setItem(SECTION_KEY, section);
  } catch {
    // Private storage: "My library" next time.
  }
}
