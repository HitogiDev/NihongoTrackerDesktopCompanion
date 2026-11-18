import { beforeEach, describe, expect, it } from 'vitest';
import type { MangaSeries } from '../../lib/types';
import { activeFilters, applyView, DEFAULT_VIEW, loadView, saveView, seriesStatus } from './libraryView';

const make = (id: string, over: Partial<MangaSeries> = {}): MangaSeries => ({
  id,
  source: 'webdav',
  title: id,
  location: `/${id}`,
  media_id: null,
  media_title: null,
  is_adult: false,
  created_at: '2026-01-01T00:00:00Z',
  volume_count: 3,
  page_count: 600,
  has_text: false,
  volumes_finished: 0,
  pages_read: 0,
  last_read_at: null,
  ...over,
});

const library = [
  make('Vol 2', { has_text: true, volume_count: 2, created_at: '2026-03-01T00:00:00Z' }),
  make('Vol 10', { media_id: '30104', pages_read: 40, last_read_at: '2026-05-02T10:00:00Z' }),
  make('Akira', { volume_count: 6, volumes_finished: 6, pages_read: 1200, last_read_at: '2026-04-01T10:00:00Z', created_at: '2026-02-01T00:00:00Z' }),
  make('blame!', { has_text: true }),
];
const ids = (list: MangaSeries[]) => list.map((item) => item.id);

describe('library view', () => {
  it('sorts titles the way people number them, both ways', () => {
    expect(ids(applyView(library, DEFAULT_VIEW))).toEqual(['Akira', 'blame!', 'Vol 2', 'Vol 10']);
    expect(ids(applyView(library, { ...DEFAULT_VIEW, sort: 'title-desc' }))).toEqual(['Vol 10', 'Vol 2', 'blame!', 'Akira']);
  });

  it('puts the last read first and the never read last, by title', () => {
    expect(ids(applyView(library, { ...DEFAULT_VIEW, sort: 'last-read' }))).toEqual(['Vol 10', 'Akira', 'blame!', 'Vol 2']);
  });

  it('sorts by when a series was added and by its volumes', () => {
    expect(ids(applyView(library, { ...DEFAULT_VIEW, sort: 'added' }))[0]).toBe('Vol 2');
    expect(ids(applyView(library, { ...DEFAULT_VIEW, sort: 'volumes' }))).toEqual(['Akira', 'blame!', 'Vol 10', 'Vol 2']);
  });

  it('filters by text, link and reading status', () => {
    expect(ids(applyView(library, { ...DEFAULT_VIEW, text: 'with' }))).toEqual(['blame!', 'Vol 2']);
    expect(ids(applyView(library, { ...DEFAULT_VIEW, text: 'without' }))).toEqual(['Akira', 'Vol 10']);
    expect(ids(applyView(library, { ...DEFAULT_VIEW, link: 'linked' }))).toEqual(['Vol 10']);
    expect(ids(applyView(library, { ...DEFAULT_VIEW, link: 'unlinked' }))).toHaveLength(3);
    expect(ids(applyView(library, { ...DEFAULT_VIEW, link: 'suggested' }, new Set(['Akira', 'Vol 10'])))).toEqual(['Akira']);
    expect(ids(applyView(library, { ...DEFAULT_VIEW, status: 'finished' }))).toEqual(['Akira']);
    expect(ids(applyView(library, { ...DEFAULT_VIEW, status: 'reading' }))).toEqual(['Vol 10']);
    expect(ids(applyView(library, { ...DEFAULT_VIEW, status: 'unstarted' }))).toEqual(['blame!', 'Vol 2']);
    expect(seriesStatus(make('empty', { volume_count: 0 }))).toBe('unstarted');
  });

  it('never reorders the list it was given', () => {
    const copy = [...library];
    applyView(library, { ...DEFAULT_VIEW, sort: 'volumes' });
    expect(library).toEqual(copy);
  });

  it('counts the filters, not the order', () => {
    expect(activeFilters({ ...DEFAULT_VIEW, sort: 'added' })).toBe(0);
    expect(activeFilters({ sort: 'added', text: 'with', link: 'suggested', status: 'any' })).toBe(2);
  });

  describe('storage', () => {
    beforeEach(() => localStorage.clear());
    it('remembers the view and ignores what it does not know', () => {
      saveView({ sort: 'volumes', text: 'with', link: 'linked', status: 'reading' });
      expect(loadView()).toEqual({ sort: 'volumes', text: 'with', link: 'linked', status: 'reading' });
      localStorage.setItem('manga:library:view', JSON.stringify({ sort: 'nope', text: 'with', status: 42 }));
      expect(loadView()).toEqual({ ...DEFAULT_VIEW, text: 'with' });
      localStorage.setItem('manga:library:view', '{broken');
      expect(loadView()).toEqual(DEFAULT_VIEW);
    });
  });
});

describe('option counts', () => {
  it('counts each choice with the other filters applied', async () => {
    const { optionCounts, TEXT_FILTERS } = await import('./libraryView');
    const view = { ...DEFAULT_VIEW, status: 'unstarted' as const };
    expect(optionCounts(library, view, 'text', TEXT_FILTERS, new Set())).toEqual({ any: 2, with: 2, without: 0 });
  });
});
