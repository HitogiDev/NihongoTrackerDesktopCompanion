import { useEffect, useRef, useState } from 'react';
import { Link } from 'react-router-dom';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { Compass, Library, Puzzle, Store } from 'lucide-react';
import { Button } from '../../../components/ui/Button';
import { EmptyState, ErrorState } from '../../../components/ui/Empty';
import { ipc } from '../../../lib/ipc';
import { formatCount } from '../../../lib/manga';
import type { MangaSeries } from '../../../lib/types';
import { useAppStore } from '../../../store/useAppStore';
import { SeriesGrid } from '../SeriesGrid';
import { ExtInstallModal } from './ExtInstallModal';
import { ExtServerStrip, ExtSetup } from './ExtServer';
import { ContentBadge, ExtIcon, ExtStore, languageName } from './ExtStore';
import { canConnect, EXT_SOURCES_QUERY, EXT_STATUS_QUERY, extErrorText, useExtStatus } from './useExtensions';

type View = 'library' | 'explore' | 'store';
const VIEWS: { id: View; icon: typeof Library }[] = [
  { id: 'library', icon: Library },
  { id: 'explore', icon: Compass },
  { id: 'store', icon: Store },
];
const VIEW_KEY = 'manga:ext:view';

function storedView(): View {
  try {
    const value = localStorage.getItem(VIEW_KEY);
    return value === 'explore' || value === 'store' || value === 'library' ? value : 'explore';
  } catch {
    return 'explore';
  }
}

/** The sources of the installed extensions, to browse. */
function Sources({ onStore }: { onStore: () => void }) {
  const { t, i18n } = useTranslation('manga');
  const showAdult = useAppStore((s) => s.settings?.manga_ext_show_adult ?? false);
  const sources = useQuery({ queryKey: [...EXT_SOURCES_QUERY, showAdult], queryFn: ipc.mangaExtSources });
  if (sources.isPending) {
    return (
      <ul className="grid grid-cols-[repeat(auto-fill,minmax(15rem,1fr))] gap-3" aria-busy="true">
        {Array.from({ length: 6 }, (_, index) => <li key={index} className="skeleton h-16 rounded-box bg-base-100" />)}
      </ul>
    );
  }
  if (sources.isError) {
    return <div className="surface"><ErrorState title={t('ext.sources.loadFailed')} error={extErrorText(sources.error)} retrying={sources.isFetching} onRetry={() => void sources.refetch()} /></div>;
  }
  if (sources.data.length === 0) {
    return (
      <div className="surface">
        <EmptyState
          icon={Compass}
          title={t('ext.sources.none')}
          description={t('ext.sources.noneBody')}
          action={<Button variant="primary" size="sm" onClick={onStore}><Store className="h-4 w-4" />{t('ext.sources.openStore')}</Button>}
        />
      </div>
    );
  }
  return (
    <ul className="grid grid-cols-[repeat(auto-fill,minmax(15rem,1fr))] gap-3">
      {sources.data.map((source) => (
        <li key={source.id}>
          <Link
            to={`/manga/ext/source/${encodeURIComponent(source.id)}`}
            state={{ name: source.display_name }}
            className="surface group flex items-center gap-3 p-3 motion-safe:transition hover:shadow-lg motion-safe:hover:-translate-y-0.5 focus-visible:outline-offset-2"
          >
            <ExtIcon token={source.icon} />
            <div className="min-w-0 flex-1">
              <p className="truncate text-sm font-semibold">{source.name}</p>
              <p className="mt-0.5 flex items-center gap-1.5 text-xs text-base-content/55">
                {languageName(source.lang, i18n.language)}
                <ContentBadge content={source.content} />
              </p>
            </div>
            <Compass className="h-4 w-4 shrink-0 text-base-content/30 motion-safe:transition-colors group-hover:text-base-content/70" aria-hidden="true" />
          </Link>
        </li>
      ))}
    </ul>
  );
}

/**
 * The Extensions tab of the Manga screen: setting the server up if it is not,
 * then the series added from extensions, the sources to explore and the
 * store. Opening it starts the server (only once it is installed or saved).
 */
export function ExtensionsPanel({ series }: { series: MangaSeries[] }) {
  const { t, i18n } = useTranslation('manga');
  const queryClient = useQueryClient();
  const status = useExtStatus();
  const [view, setView] = useState<View>(storedView);
  const [starting, setStarting] = useState(false);
  const asked = useRef(false);
  const data = status.data;
  const connectable = canConnect(data);

  const start = async () => {
    setStarting(true);
    try {
      queryClient.setQueryData(EXT_STATUS_QUERY, await ipc.mangaExtStart());
    } catch {
      await queryClient.invalidateQueries({ queryKey: EXT_STATUS_QUERY });
    } finally {
      setStarting(false);
    }
  };

  // Using extensions is what starts the server; never before it is set up.
  useEffect(() => {
    if (connectable && data?.phase === 'stopped' && !asked.current) {
      asked.current = true;
      void start();
    }
  }, [connectable, data?.phase]);

  const choose = (next: View) => {
    setView(next);
    try { localStorage.setItem(VIEW_KEY, next); } catch { /* private storage */ }
  };

  if (status.isPending) return <div className="skeleton h-40 rounded-box bg-base-100" />;
  if (status.isError || !data) return <div className="surface"><ErrorState title={t('ext.loadFailed')} error={status.error} onRetry={() => void status.refetch()} /></div>;

  if (!connectable) {
    return (
      <>
        <ExtSetup status={data} />
        {series.length > 0 ? (
          <section className="space-y-3">
            <h2 className="text-[11px] uppercase tracking-[0.2em] text-base-content/60">{t('ext.view.library')}</h2>
            <SeriesGrid series={series} />
          </section>
        ) : null}
        <ExtInstallModal />
      </>
    );
  }

  const ready = data.phase === 'ready';
  return (
    <>
      <ExtServerStrip status={data} retrying={starting} onRetry={() => void start()} />
      <div role="tablist" aria-label={t('ext.view.label')} className="tabs tabs-border">
        {VIEWS.map(({ id, icon: Icon }) => (
          <button key={id} type="button" role="tab" aria-selected={view === id} className={view === id ? 'tab tab-active gap-2' : 'tab gap-2'} onClick={() => choose(id)}>
            <Icon className="h-4 w-4" aria-hidden="true" />
            {t(`ext.view.${id}`)}
            {id === 'library' && series.length > 0 ? <span className="badge badge-ghost badge-xs">{formatCount(series.length, i18n.language)}</span> : null}
          </button>
        ))}
      </div>
      {view === 'library' ? (
        series.length > 0 ? (
          <SeriesGrid series={series} />
        ) : (
          <div className="surface">
            <EmptyState icon={Library} title={t('ext.library.none')} description={t('ext.library.noneBody')} action={<Button size="sm" variant="primary" onClick={() => choose('explore')}><Compass className="h-4 w-4" />{t('ext.library.explore')}</Button>} />
          </div>
        )
      ) : !ready ? (
        <div className="surface">
          <EmptyState icon={Puzzle} title={t(`ext.server.phase.${data.phase}`)} description={data.phase === 'starting' ? t('ext.server.startingBody') : undefined} />
        </div>
      ) : view === 'explore' ? (
        <Sources onStore={() => choose('store')} />
      ) : (
        <ExtStore />
      )}
      <ExtInstallModal />
    </>
  );
}
