import { useEffect, useMemo, useRef, useState } from 'react';
import { useLocation } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { ArrowLeft, Bookmark, FolderCog, FolderPlus, Keyboard, Library, ListFilter, RefreshCw, Search, SearchX, Server, Sparkles } from 'lucide-react';
import { MANGA } from './manga/Cover';
import { Button } from '../components/ui/Button';
import { EmptyState, ErrorState } from '../components/ui/Empty';
import { Page, SectionTitle } from '../components/ui/Page';
import { EmptyLibrary } from './manga/EmptyLibrary';
import { ShortcutsSheet } from './manga/ShortcutsSheet';
import { isSheetKey, isTyping, libraryShortcuts } from './manga/shortcuts';
import { FoldersModal } from './manga/FoldersModal';
import { ServerFormModal } from './manga/ServerFormModal';
import { ServersModal, ServerStrip, SyncRing, useWebdavSyncSummary } from './manga/Servers';
import { ExtensionsPanel } from './manga/ext/ExtensionsPanel';
import { LibraryMenu, type Origin } from './manga/LibraryMenu';
import { useWebdavSyncEvents } from './manga/useWebdav';
import { ContinueShelf, SeriesGrid, SeriesGridSkeleton, ShelfSkeleton } from './manga/SeriesGrid';
import { useAddFolder, useRescan } from './manga/useAddFolder';
import { useMangaLibrary, useMangaLibraryEvents } from './manga/useMangaLibrary';
import { formatCount } from '../lib/manga';
import { LibraryToolbar } from './manga/LibraryToolbar';
import { SuggestionReview } from './manga/Suggestions';
import { useSuggestions } from './manga/useSuggestions';
import { activeFilters, applyView, cleared, loadSection, loadView, saveSection, saveView, type LibraryView, type Section } from './manga/libraryView';
import type { MangaSource } from '../lib/types';

const SECTIONS: Section[] = ['library', 'all'];

export default function MangaScreen() {
  const { t, i18n } = useTranslation(['manga', 'common']);
  useMangaLibraryEvents();
  useWebdavSyncEvents();
  const library = useMangaLibrary();
  const { add, adding } = useAddFolder();
  const { rescan, scanning } = useRescan();
  // Back from a source or a series of one: the extensions again.
  const routeState = useLocation().state as { origin?: Origin } | null;
  const [extensions, setExtensions] = useState(routeState?.origin === 'extensions');
  // "My library" (what the user keeps) or "All series" (every source's
  // catalogue), remembered with a sort and filters of its own each.
  const [section, setSectionState] = useState<Section>(loadSection);
  const setSection = (next: Section) => { setSectionState(next); saveSection(next); setExtensions(false); };
  const [search, setSearch] = useState('');
  const [views, setViews] = useState<Record<Section, LibraryView>>(() => ({ library: loadView('library'), all: loadView('all') }));
  const view = views[section];
  const setView = (next: LibraryView) => { setViews((current) => ({ ...current, [section]: next })); saveView(section, next); };
  const origin: Origin = extensions ? 'extensions' : 'all';
  const [foldersOpen, setFoldersOpen] = useState(false);
  const [serversOpen, setServersOpen] = useState(false);
  // `undefined`: the form is closed; `null`: a new server.
  const [editing, setEditing] = useState<MangaSource | null | undefined>(undefined);
  const openForm = (source: MangaSource | null) => { setServersOpen(false); setEditing(source); };
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const shortcutGroups = useMemo(() => libraryShortcuts(), []);
  const searchField = useRef<HTMLInputElement>(null);

  // `?` lists the shortcuts; `/` jumps to the search, as on the web.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (document.querySelector('dialog[open] .modal-box:not(.manga-shortcuts)')) return;
      if (isSheetKey(event)) {
        event.preventDefault();
        setShortcutsOpen((open) => !open);
      } else if (event.key === '/' && !event.ctrlKey && !event.metaKey && !event.altKey && !isTyping(event.target) && searchField.current) {
        event.preventDefault();
        searchField.current.focus();
        searchField.current.select();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  // Opening the screen picks up what changed on disk since (at most once a
  // minute; Rust decides).
  useEffect(() => { void rescan(true); }, []);

  const data = library.data;
  const inLibrary = useMemo(() => (data?.series ?? []).filter((series) => series.in_library_at), [data]);
  const pool = section === 'library' ? inLibrary : data?.series ?? [];
  // The section and the search, then the view's filters and order.
  const searched = useMemo(() => {
    const needle = search.trim().toLowerCase();
    return pool.filter((series) => !needle || `${series.title} ${series.media_title ?? ''}`.toLowerCase().includes(needle));
  }, [pool, search]);
  // Suggestions are for the manga the user keeps: "My library" only. In
  // "All series" a catalogue of a thousand would be a thousand badges.
  const suggestions = useSuggestions();
  const suggested = useMemo(
    () => (section === 'library' ? new Set(inLibrary.filter((item) => suggestions.byId.has(item.id)).map((item) => item.id)) : new Set<string>()),
    [section, inLibrary, suggestions.byId],
  );
  const filtered = useMemo(() => applyView(searched, view, suggested), [searched, view, suggested]);
  const narrowed = activeFilters(view) > 0;
  const reviewing = section === 'library' && view.link === 'suggested';
  const waiting = useMemo(() => searched.filter((item) => !item.media_id && suggested.has(item.id)).length, [searched, suggested]);

  // The extensions server's own rows do not make a library: only its series do.
  const hasSources = (data?.sources ?? []).some((source) => source.kind !== 'suwayomi' || source.series_count > 0) || extensions;
  const hasSeries = (data?.series.length ?? 0) > 0;
  const servers = useMemo(() => (data?.sources ?? []).filter((source) => source.kind === 'webdav'), [data]);
  const filtering = search.trim() !== '' || narrowed;
  // A sync is background work: a small ring in the header, whatever is open.
  const syncing = useWebdavSyncSummary(servers);
  const total = (count: number) => formatCount(count, i18n.language);

  return (
    <Page
      title={t('title')}
      description={t('subtitle')}
      actions={
        hasSources ? (
          <>
            <Button appearance="ghost" size="sm" shape="square" aria-label={t('mangaReader:shortcuts.title')} title={t('mangaReader:shortcuts.open')} onClick={() => setShortcutsOpen(true)}>
              <Keyboard className="h-4 w-4" />
            </Button>
            {syncing ? (
              <button type="button" className="btn btn-ghost btn-sm gap-1.5 px-2" onClick={() => setServersOpen(true)} aria-label={t('menu.manageServers')}>
                <SyncRing done={syncing.done} total={syncing.total} />
              </button>
            ) : null}
            {extensions ? null : (
              <Button appearance="ghost" size="sm" shape="square" loading={scanning} aria-label={t('actions.rescan')} title={t('actions.rescan')} onClick={() => void rescan()}>
                {scanning ? null : <RefreshCw className="h-4 w-4" />}
              </Button>
            )}
            <LibraryMenu
              origin={origin}
              sources={data?.sources ?? []}
              adding={adding}
              onFolders={() => setFoldersOpen(true)}
              onAddFolder={() => void add()}
              onServers={() => setServersOpen(true)}
              onAddServer={() => openForm(null)}
              onExtensions={() => setExtensions(true)}
            />
          </>
        ) : null
      }
    >
      {library.isPending ? (
        <>
          <div className="space-y-3"><SectionTitle>{t('shelf.title')}</SectionTitle><ShelfSkeleton /></div>
          <div className="space-y-3"><SectionTitle>{t(`sections.${section}`)}</SectionTitle><SeriesGridSkeleton /></div>
        </>
      ) : library.isError ? (
        <div className="surface">
          <ErrorState title={t('loadFailed')} error={library.error} retrying={library.isFetching} onRetry={() => void library.refetch()} />
        </div>
      ) : !hasSources ? (
        <EmptyLibrary onAdd={() => void add()} adding={adding} onAddServer={() => openForm(null)} onExtensions={() => setExtensions(true)} />
      ) : (
        <>
          {extensions ? (
            <div>
              <button type="button" className="btn btn-ghost btn-sm -ml-2 gap-1.5" onClick={() => setExtensions(false)}>
                <ArrowLeft className="h-4 w-4" aria-hidden="true" />
                {t(`sections.${section}`)}
              </button>
            </div>
          ) : (
            <div className="flex flex-wrap items-center gap-3">
              <div role="tablist" aria-label={t('sections.label')} className="tabs tabs-box tabs-sm">
                {SECTIONS.map((id) => (
                  <button
                    key={id}
                    type="button"
                    role="tab"
                    aria-selected={section === id}
                    className={section === id ? 'tab tab-active gap-1.5' : 'tab gap-1.5'}
                    onClick={() => setSection(id)}
                  >
                    {id === 'library' ? <Bookmark className="h-3.5 w-3.5" aria-hidden="true" /> : <Library className="h-3.5 w-3.5" aria-hidden="true" />}
                    {t(`sections.${id}`)}
                    <span className="text-xs tabular-nums opacity-60">{total(id === 'library' ? inLibrary.length : data?.series.length ?? 0)}</span>
                  </button>
                ))}
              </div>
              <label className="input input-sm min-w-48 flex-1">
                <Search className="h-4 w-4 opacity-60" />
                <input ref={searchField} type="search" aria-keyshortcuts="/" value={search} onChange={(event) => setSearch(event.target.value)} placeholder={section === 'library' ? t('searchLibrary') : t('search')} />
              </label>
            </div>
          )}

          {!extensions && section === 'all' && view.origin === 'webdav' && servers.length > 0 ? <ServerStrip sources={servers} onManage={() => setServersOpen(true)} /> : null}

          {extensions ? <ExtensionsPanel series={(data?.series ?? []).filter((series) => series.source === 'suwayomi')} /> : null}

          {!extensions && section === 'library' && !filtering && data && data.continue_reading.length > 0 ? (
            <section className="space-y-3" aria-label={t('shelf.title')}>
              <SectionTitle>{t('shelf.title')}</SectionTitle>
              <ContinueShelf items={data.continue_reading} inLibrary={(id) => data.series.find((series) => series.id === id)?.in_library_at ?? null} />
            </section>
          ) : null}

          {extensions ? null : <section className="space-y-3" aria-label={t(`sections.${section}`)}>
            <SectionTitle
              aside={
                <div className="flex flex-wrap items-center justify-end gap-x-3 gap-y-2">
                  <span className="text-xs tabular-nums text-base-content/50">
                    {narrowed ? t('view.shown', { shown: total(filtered.length), total: total(searched.length) }) : total(filtered.length)}
                  </span>
                  {reviewing ? (
                    <button type="button" className="btn btn-ghost btn-sm gap-1.5" onClick={() => setView({ ...view, link: 'any' })}>
                      <ArrowLeft className="h-4 w-4" aria-hidden="true" />
                      {t('suggest.review.back')}
                    </button>
                  ) : waiting > 0 ? (
                    <button type="button" className="btn btn-ghost btn-sm gap-1.5" onClick={() => setView({ ...view, link: 'suggested' })} title={t('suggest.review.open')}>
                      <Sparkles className={`h-4 w-4 ${MANGA.color}`} aria-hidden="true" />
                      {t('suggest.review.waiting', { count: waiting })}
                    </button>
                  ) : null}
                  {pool.length > 0 ? <LibraryToolbar series={searched} view={view} onChange={setView} suggested={suggested} withSuggestions={section === 'library'} /> : null}
                </div>
              }
            >
              {reviewing ? t('suggest.review.title') : section === 'library' ? t('library.gridTitle') : t('grid.allTitle')}
            </SectionTitle>
            {reviewing && searched.length > 0 ? (
              <SuggestionReview series={filtered} suggestions={suggestions.byId} />
            ) : filtered.length > 0 ? (
              <SeriesGrid series={filtered} suggested={suggested} toggle={section === 'all'} />
            ) : narrowed && searched.length > 0 ? (
              <div className="surface">
                <EmptyState
                  icon={ListFilter}
                  title={t('view.empty')}
                  description={t('view.emptyBody')}
                  action={<Button size="sm" appearance="outline" onClick={() => setView(cleared(view))}>{t('view.clear')}</Button>}
                />
              </div>
            ) : section === 'library' && inLibrary.length === 0 && hasSeries ? (
              <div className="surface">
                <EmptyState
                  icon={Bookmark}
                  title={t('library.empty.title')}
                  description={t('library.empty.body')}
                  action={
                    <Button size="sm" variant="primary" onClick={() => setSection('all')}>
                      <Library className="h-4 w-4" />
                      {t('library.empty.action')}
                    </Button>
                  }
                />
              </div>
            ) : view.origin === 'webdav' && servers.length === 0 ? (
              <div className="surface">
                <EmptyState
                  icon={Server}
                  title={t('webdav.empty.title')}
                  description={t('webdav.empty.body')}
                  action={
                    <Button size="sm" variant="primary" onClick={() => openForm(null)}>
                      <Server className="h-4 w-4" />
                      {t('webdav.actions.add')}
                    </Button>
                  }
                />
              </div>
            ) : view.origin === 'webdav' && search.trim() === '' && searched.length === 0 ? (
              <div className="surface">
                <EmptyState icon={Server} title={t('webdav.empty.noSeries')} description={t('webdav.empty.noSeriesBody')} />
              </div>
            ) : hasSeries ? (
              <div className="surface">
                <EmptyState
                  icon={SearchX}
                  title={t('grid.noMatches')}
                  action={<Button size="sm" appearance="outline" onClick={() => { setSearch(''); setView(cleared(view)); }}>{t('grid.clear')}</Button>}
                />
              </div>
            ) : (
              <div className="surface">
                <EmptyState
                  icon={FolderPlus}
                  title={t('grid.nothingFound')}
                  description={t('grid.nothingFoundBody')}
                  action={
                    <div className="flex flex-wrap justify-center gap-2">
                      <Button size="sm" appearance="outline" loading={scanning} onClick={() => void rescan()}>
                        {scanning ? null : <RefreshCw className="h-4 w-4" />}
                        {t('actions.rescan')}
                      </Button>
                      <Button size="sm" appearance="ghost" onClick={() => setFoldersOpen(true)}>
                        <FolderCog className="h-4 w-4" />
                        {t('menu.manageFolders')}
                      </Button>
                    </div>
                  }
                />
              </div>
            )}
          </section>}
        </>
      )}
      <ServersModal open={serversOpen} onClose={() => setServersOpen(false)} sources={data?.sources ?? []} onAdd={() => openForm(null)} onEdit={openForm} />
      <ServerFormModal open={editing !== undefined} onClose={() => setEditing(undefined)} source={editing} />
      <ShortcutsSheet open={shortcutsOpen} onClose={() => setShortcutsOpen(false)} groups={shortcutGroups} note={t('mangaReader:shortcuts.noteLibrary')} />
      <FoldersModal open={foldersOpen} onClose={() => setFoldersOpen(false)} sources={data?.sources ?? []} onAdd={() => void add()} adding={adding} />
    </Page>
  );
}
