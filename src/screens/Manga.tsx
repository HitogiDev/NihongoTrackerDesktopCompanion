import { useEffect, useMemo, useRef, useState } from 'react';
import { useLocation } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { FolderCog, FolderPlus, Keyboard, ListFilter, RefreshCw, Search, SearchX, Server, Sparkles } from 'lucide-react';
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
import { ipc } from '../lib/ipc';
import { activeFilters, applyView, DEFAULT_VIEW, loadView, saveView, type LibraryView } from './manga/libraryView';
import type { MangaSource } from '../lib/types';

const ORIGINS: Origin[] = ['all', 'local', 'webdav', 'extensions'];
const ORIGIN_SOURCE: Record<Origin, string | null> = { all: null, local: 'local', webdav: 'webdav', extensions: 'suwayomi' };

export default function MangaScreen() {
  const { t, i18n } = useTranslation(['manga', 'common']);
  useMangaLibraryEvents();
  useWebdavSyncEvents();
  const library = useMangaLibrary();
  const { add, adding } = useAddFolder();
  const { rescan, scanning } = useRescan();
  // Back from a source or a series of one: the Extensions tab again.
  const routeState = useLocation().state as { origin?: Origin } | null;
  const [origin, setOrigin] = useState<Origin>(routeState?.origin ?? 'all');
  const [search, setSearch] = useState('');
  // Sort and filters: one choice for every tab, remembered.
  const [view, setViewState] = useState<LibraryView>(loadView);
  const setView = (next: LibraryView) => { setViewState(next); saveView(next); };
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
  // The tab and the search, then the view's filters and order.
  const searched = useMemo(() => {
    const needle = search.trim().toLowerCase();
    const source = ORIGIN_SOURCE[origin];
    return (data?.series ?? []).filter((series) =>
      (!source || series.source === source)
      && (!needle || `${series.title} ${series.media_title ?? ''}`.toLowerCase().includes(needle)));
  }, [data, origin, search]);
  const suggestions = useSuggestions();
  const suggested = useMemo(() => new Set(suggestions.byId.keys()), [suggestions.byId]);
  const filtered = useMemo(() => applyView(searched, view, suggested), [searched, view, suggested]);
  const narrowed = activeFilters(view) > 0;
  const reviewing = view.link === 'suggested';
  const waiting = useMemo(() => searched.filter((item) => !item.media_id && suggested.has(item.id)).length, [searched, suggested]);

  // Suggestions look up first what the grid shows first (Rust keeps the pace).
  useEffect(() => {
    const ids = filtered.filter((item) => !item.media_id && !suggested.has(item.id)).slice(0, 240).map((item) => item.id);
    if (ids.length === 0) return;
    const timer = setTimeout(() => void ipc.mangaSuggestPriority(ids).catch(() => undefined), 800);
    return () => clearTimeout(timer);
  }, [filtered, suggested]);

  // The extensions server's own rows do not make a library: only its series do.
  const hasSources = (data?.sources ?? []).some((source) => source.kind !== 'suwayomi' || source.series_count > 0) || origin === 'extensions';
  const hasSeries = (data?.series.length ?? 0) > 0;
  const servers = useMemo(() => (data?.sources ?? []).filter((source) => source.kind === 'webdav'), [data]);
  const filtering = search.trim() !== '' || origin !== 'all' || narrowed;
  // A sync is background work: a ring on the WebDAV tab, whatever tab is open.
  const syncing = useWebdavSyncSummary(servers);

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
            {origin === 'extensions' ? null : (
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
              onExtensions={() => setOrigin('extensions')}
            />
          </>
        ) : null
      }
    >
      {library.isPending ? (
        <>
          <div className="space-y-3"><SectionTitle>{t('shelf.title')}</SectionTitle><ShelfSkeleton /></div>
          <div className="space-y-3"><SectionTitle>{t('grid.title')}</SectionTitle><SeriesGridSkeleton /></div>
        </>
      ) : library.isError ? (
        <div className="surface">
          <ErrorState title={t('loadFailed')} error={library.error} retrying={library.isFetching} onRetry={() => void library.refetch()} />
        </div>
      ) : !hasSources ? (
        <EmptyLibrary onAdd={() => void add()} adding={adding} onAddServer={() => openForm(null)} onExtensions={() => setOrigin('extensions')} />
      ) : (
        <>
          <div className="flex flex-wrap items-center gap-3">
            <label className="input input-sm min-w-48 flex-1">
              <Search className="h-4 w-4 opacity-60" />
              <input ref={searchField} type="search" aria-keyshortcuts="/" value={search} onChange={(event) => setSearch(event.target.value)} placeholder={t('search')} />
            </label>
            {origin !== 'extensions' && waiting > 0 && !reviewing ? (
              <button type="button" className="btn btn-ghost btn-sm gap-1.5 text-base-content/70" onClick={() => setView({ ...view, link: 'suggested' })} title={t('suggest.review.open')}>
                <Sparkles className={`h-4 w-4 ${MANGA.color}`} aria-hidden="true" />
                {t('suggest.review.waiting', { count: waiting })}
              </button>
            ) : null}
            {origin === 'extensions' ? null : <LibraryToolbar series={searched} view={view} onChange={setView} suggested={suggested} />}
            <div role="tablist" aria-label={t('tabs.label')} className="tabs tabs-box tabs-sm">
              {ORIGINS.map((id) => (
                <button
                  key={id}
                  type="button"
                  role="tab"
                  aria-selected={origin === id}
                  className={origin === id ? 'tab tab-active gap-1.5' : 'tab gap-1.5'}
                  onClick={() => setOrigin(id)}
                >
                  {t(`tabs.${id}`)}
                  {id === 'webdav' && syncing ? <SyncRing done={syncing.done} total={syncing.total} /> : null}
                </button>
              ))}
            </div>
          </div>

          {origin === 'webdav' && servers.length > 0 ? <ServerStrip sources={servers} onManage={() => setServersOpen(true)} /> : null}

          {origin === 'extensions' ? <ExtensionsPanel series={searched} /> : null}

          {origin !== 'extensions' && !filtering && data && data.continue_reading.length > 0 ? (
            <section className="space-y-3" aria-label={t('shelf.title')}>
              <SectionTitle>{t('shelf.title')}</SectionTitle>
              <ContinueShelf items={data.continue_reading} />
            </section>
          ) : null}

          {origin === 'extensions' ? null : <section className="space-y-3" aria-label={t('grid.title')}>
            <SectionTitle aside={<span className="text-xs tabular-nums text-base-content/50">{narrowed ? t('view.shown', { shown: formatCount(filtered.length, i18n.language), total: formatCount(searched.length, i18n.language) }) : formatCount(filtered.length, i18n.language)}</span>}>{reviewing ? t('suggest.review.title') : t('grid.title')}</SectionTitle>
            {reviewing && searched.length > 0 ? (
              <SuggestionReview series={filtered} suggestions={suggestions.byId} />
            ) : filtered.length > 0 ? (
              <SeriesGrid series={filtered} suggested={suggested} />
            ) : narrowed && searched.length > 0 ? (
              <div className="surface">
                <EmptyState
                  icon={ListFilter}
                  title={t('view.empty')}
                  description={t('view.emptyBody')}
                  action={<Button size="sm" appearance="outline" onClick={() => setView({ ...DEFAULT_VIEW, sort: view.sort })}>{t('view.clear')}</Button>}
                />
              </div>
            ) : origin === 'webdav' && servers.length === 0 ? (
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
            ) : origin === 'webdav' && search.trim() === '' ? (
              <div className="surface">
                <EmptyState icon={Server} title={t('webdav.empty.noSeries')} description={t('webdav.empty.noSeriesBody')} />
              </div>
            ) : hasSeries ? (
              <div className="surface">
                <EmptyState
                  icon={SearchX}
                  title={t('grid.noMatches')}
                  action={<Button size="sm" appearance="outline" onClick={() => { setSearch(''); setOrigin('all'); setView({ ...DEFAULT_VIEW, sort: view.sort }); }}>{t('grid.clear')}</Button>}
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
