import { useEffect, useMemo, useState } from 'react';
import { useLocation } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { FolderCog, FolderPlus, RefreshCw, Search, SearchX, Server } from 'lucide-react';
import { Button } from '../components/ui/Button';
import { EmptyState, ErrorState } from '../components/ui/Empty';
import { Page, SectionTitle } from '../components/ui/Page';
import { EmptyLibrary } from './manga/EmptyLibrary';
import { FoldersModal } from './manga/FoldersModal';
import { ServerFormModal } from './manga/ServerFormModal';
import { ServersModal, ServerStrip } from './manga/Servers';
import { ExtensionsPanel } from './manga/ext/ExtensionsPanel';
import { useWebdavSyncEvents } from './manga/useWebdav';
import { ContinueShelf, SeriesGrid, SeriesGridSkeleton, ShelfSkeleton } from './manga/SeriesGrid';
import { useAddFolder, useRescan } from './manga/useAddFolder';
import { useMangaLibrary, useMangaLibraryEvents } from './manga/useMangaLibrary';
import { formatCount } from '../lib/manga';
import type { MangaSource } from '../lib/types';

type Origin = 'all' | 'local' | 'webdav' | 'extensions';
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
  const [foldersOpen, setFoldersOpen] = useState(false);
  const [serversOpen, setServersOpen] = useState(false);
  // `undefined`: the form is closed; `null`: a new server.
  const [editing, setEditing] = useState<MangaSource | null | undefined>(undefined);
  const openForm = (source: MangaSource | null) => { setServersOpen(false); setEditing(source); };

  // Opening the screen picks up what changed on disk since (at most once a
  // minute; Rust decides).
  useEffect(() => { void rescan(true); }, []);

  const data = library.data;
  const filtered = useMemo(() => {
    const needle = search.trim().toLowerCase();
    const source = ORIGIN_SOURCE[origin];
    return (data?.series ?? []).filter((series) =>
      (!source || series.source === source)
      && (!needle || `${series.title} ${series.media_title ?? ''}`.toLowerCase().includes(needle)));
  }, [data, origin, search]);

  // The extensions server's own rows do not make a library: only its series do.
  const hasSources = (data?.sources ?? []).some((source) => source.kind !== 'suwayomi' || source.series_count > 0) || origin === 'extensions';
  const hasSeries = (data?.series.length ?? 0) > 0;
  const servers = useMemo(() => (data?.sources ?? []).filter((source) => source.kind === 'webdav'), [data]);
  const filtering = search.trim() !== '' || origin !== 'all';

  return (
    <Page
      title={t('title')}
      description={t('subtitle')}
      actions={
        hasSources ? (
          <>
            <Button appearance="ghost" size="sm" shape="square" loading={scanning} aria-label={t('actions.rescan')} title={t('actions.rescan')} onClick={() => void rescan()}>
              {scanning ? null : <RefreshCw className="h-4 w-4" />}
            </Button>
            <Button appearance="ghost" size="sm" onClick={() => setFoldersOpen(true)}>
              <FolderCog className="h-4 w-4" />
              {t('actions.folders')}
            </Button>
            <Button appearance="ghost" size="sm" onClick={() => (servers.length ? setServersOpen(true) : openForm(null))}>
              <Server className="h-4 w-4" />
              {t('webdav.actions.servers')}
            </Button>
            <Button variant="primary" size="sm" loading={adding} onClick={() => void add()}>
              {adding ? null : <FolderPlus className="h-4 w-4" />}
              {t('actions.addFolder')}
            </Button>
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
              <input type="search" value={search} onChange={(event) => setSearch(event.target.value)} placeholder={t('search')} />
            </label>
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
                </button>
              ))}
            </div>
          </div>

          {origin === 'webdav' && servers.length > 0 ? <ServerStrip sources={servers} onManage={() => setServersOpen(true)} /> : null}

          {origin === 'extensions' ? <ExtensionsPanel series={filtered} /> : null}

          {origin !== 'extensions' && !filtering && data && data.continue_reading.length > 0 ? (
            <section className="space-y-3" aria-label={t('shelf.title')}>
              <SectionTitle>{t('shelf.title')}</SectionTitle>
              <ContinueShelf items={data.continue_reading} />
            </section>
          ) : null}

          {origin === 'extensions' ? null : <section className="space-y-3" aria-label={t('grid.title')}>
            <SectionTitle aside={<span className="text-xs tabular-nums text-base-content/50">{formatCount(filtered.length, i18n.language)}</span>}>{t('grid.title')}</SectionTitle>
            {filtered.length > 0 ? (
              <SeriesGrid series={filtered} />
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
                  action={<Button size="sm" appearance="outline" onClick={() => { setSearch(''); setOrigin('all'); }}>{t('grid.clear')}</Button>}
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
                        {t('actions.folders')}
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
      <FoldersModal open={foldersOpen} onClose={() => setFoldersOpen(false)} sources={data?.sources ?? []} onAdd={() => void add()} adding={adding} />
    </Page>
  );
}
