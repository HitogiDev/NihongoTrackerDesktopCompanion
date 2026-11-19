import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { CloudOff, Pencil, Plus, RefreshCw, Server, ShieldAlert, Trash2 } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Modal } from '../../components/ui/Modal';
import { ipc } from '../../lib/ipc';
import { displayUrl, formatCount, relativeTime } from '../../lib/manga';
import type { MangaSource } from '../../lib/types';
import { toast } from '../../store/useToastStore';
import { MANGA } from './Cover';
import { syncSource, useWebdavSyncStore } from './useWebdav';

/**
 * A small ring that fills as a sync lists series, for the WebDAV tab and a
 * server's own line: the sync is background work, never the main thing on
 * screen. Without a total yet it spins.
 */
export function SyncRing({ done, total, className = '' }: { done: number; total: number; className?: string }) {
  const { t } = useTranslation('manga');
  const label = total > 0 ? t('webdav.status.syncingCount', { done, total }) : t('webdav.status.syncing');
  const fraction = total > 0 ? Math.min(1, done / total) : 0.25;
  // r = 6 on a 16 box: a circumference of 2πr.
  const length = 2 * Math.PI * 6;
  return (
    <span role="img" aria-label={label} title={label} className={`inline-grid h-3.5 w-3.5 shrink-0 place-items-center text-primary ${className}`}>
      <svg viewBox="0 0 16 16" className={total > 0 ? 'h-full w-full -rotate-90' : 'h-full w-full -rotate-90 motion-safe:animate-spin'} aria-hidden="true">
        <circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" strokeOpacity="0.2" strokeWidth="2.5" />
        <circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeDasharray={`${fraction * length} ${length}`} className="motion-safe:transition-[stroke-dasharray] motion-safe:duration-500" />
      </svg>
    </span>
  );
}

/** Every WebDAV source being listed now, added up: what the tab's ring shows. */
export function useWebdavSyncSummary(sources: MangaSource[]): { done: number; total: number } | null {
  const progress = useWebdavSyncStore((s) => s.progress);
  let running = false;
  let done = 0;
  let total = 0;
  for (const source of sources) {
    if (source.kind !== 'webdav') continue;
    const current = progress[source.id];
    if (!(current?.running ?? source.syncing)) continue;
    running = true;
    done += current?.done ?? 0;
    total += current?.total ?? 0;
  }
  return running ? { done, total } : null;
}

/** One line on how a server is doing: listing, offline, refused, or when it was last seen. */
export function ServerStatus({ source, className = '' }: { source: MangaSource; className?: string }) {
  const { t, i18n } = useTranslation('manga');
  const progress = useWebdavSyncStore((s) => s.progress[source.id]);
  const running = progress?.running ?? source.syncing;
  if (running) {
    const total = progress?.total ?? 0;
    const done = progress?.done ?? 0;
    return (
      <span className={`inline-flex items-center gap-1.5 text-xs tabular-nums text-base-content/55 ${className}`}>
        <SyncRing done={done} total={total} />
        <span aria-hidden="true">
          {total > 0
            ? t('webdav.status.syncingShort', { done: formatCount(done, i18n.language), total: formatCount(total, i18n.language) })
            : t('webdav.status.syncing')}
        </span>
      </span>
    );
  }
  if (source.last_error === 'offline') {
    return (
      <span className={`inline-flex items-center gap-1.5 text-xs text-warning ${className}`}>
        <CloudOff className="h-3.5 w-3.5" aria-hidden="true" />
        {t('webdav.status.offline')}
      </span>
    );
  }
  if (source.last_error === 'unauthorized') {
    return (
      <span className={`inline-flex items-center gap-1.5 text-xs text-error ${className}`}>
        <ShieldAlert className="h-3.5 w-3.5" aria-hidden="true" />
        {t('webdav.status.unauthorized')}
      </span>
    );
  }
  if (source.last_error) {
    return <span className={`text-xs text-error ${className}`}>{t('webdav.status.failed')}</span>;
  }
  return (
    <span className={`text-xs text-base-content/55 ${className}`}>
      {source.last_sync_at
        ? t('webdav.status.synced', { when: relativeTime(source.last_sync_at, i18n.language) })
        : t('webdav.status.never')}
    </span>
  );
}

function ServerRow({ source, onEdit }: { source: MangaSource; onEdit: () => void }) {
  const { t } = useTranslation(['manga', 'common']);
  const [removing, setRemoving] = useState(false);
  const running = useWebdavSyncStore((s) => s.progress[source.id]?.running) ?? source.syncing;

  const remove = async () => {
    setRemoving(true);
    try {
      await ipc.mangaRemoveSource(source.id);
      toast.success(t('toast.removed', { name: source.name }));
    } catch (error) {
      toast.error(t('toast.removeFailed', { message: String(error) }));
    } finally {
      setRemoving(false);
    }
  };

  return (
    <li className="flex items-center gap-3 px-4 py-3">
      <span className={`grid h-9 w-9 shrink-0 place-items-center rounded-field ${MANGA.bgColor} ${MANGA.color}`}>
        <Server className="h-4 w-4" aria-hidden="true" />
      </span>
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-medium">{source.name}</p>
        <p className="selectable truncate font-mono text-[11px] text-base-content/50" title={source.url}>{displayUrl(source.url)}</p>
        <ServerStatus source={source} className="mt-0.5" />
      </div>
      <span className="hidden shrink-0 text-xs text-base-content/50 sm:inline">{t('folders.series', { count: Number(source.series_count) })}</span>
      <div className="flex shrink-0 items-center">
        <Button appearance="ghost" size="sm" shape="square" disabled={running} aria-label={t('webdav.actions.syncAll', { name: source.name })} title={t('webdav.actions.syncAll', { name: source.name })} onClick={() => void syncSource(source.id, true)}>
          <RefreshCw className="h-4 w-4" />
        </Button>
        <Button appearance="ghost" size="sm" shape="square" aria-label={t('webdav.actions.edit', { name: source.name })} title={t('webdav.actions.edit', { name: source.name })} onClick={onEdit}>
          <Pencil className="h-4 w-4" />
        </Button>
        <Button appearance="ghost" size="sm" shape="square" loading={removing} aria-label={t('webdav.actions.remove', { name: source.name })} title={t('webdav.actions.remove', { name: source.name })} onClick={() => void remove()}>
          {removing ? null : <Trash2 className="h-4 w-4" />}
        </Button>
      </div>
    </li>
  );
}

/** The WebDAV servers: their state, sync, edit, forget, and a way to add one. */
export function ServersModal({ open, onClose, sources, onAdd, onEdit }: {
  open: boolean;
  onClose: () => void;
  sources: MangaSource[];
  onAdd: () => void;
  onEdit: (source: MangaSource) => void;
}) {
  const { t } = useTranslation('manga');
  const servers = sources.filter((source) => source.kind === 'webdav');
  return (
    <Modal
      open={open}
      onClose={onClose}
      size="lg"
      title={t('webdav.servers.title')}
      actions={
        <Button variant="primary" size="sm" onClick={onAdd}>
          <Plus className="h-4 w-4" />
          {t('webdav.actions.add')}
        </Button>
      }
    >
      <p className="-mt-2 mb-4 text-sm text-base-content/60">{t('webdav.servers.body')}</p>
      {servers.length === 0 ? (
        <p className="surface-muted px-4 py-6 text-center text-sm text-base-content/60">{t('webdav.servers.none')}</p>
      ) : (
        <ul className="surface-muted divide-y divide-base-300/60">
          {servers.map((source) => <ServerRow key={source.id} source={source} onEdit={() => onEdit(source)} />)}
        </ul>
      )}
      <p className="mt-3 text-xs text-base-content/50">{t('webdav.servers.removeHint')}</p>
    </Modal>
  );
}

/** Above the WebDAV tab: each server with its state and a sync button. */
export function ServerStrip({ sources, onManage }: { sources: MangaSource[]; onManage: () => void }) {
  const { t } = useTranslation('manga');
  const progress = useWebdavSyncStore((s) => s.progress);
  return (
    <ul className="flex flex-wrap gap-3">
      {sources.map((source) => {
        const running = progress[source.id]?.running ?? source.syncing;
        const offline = !running && source.last_error === 'offline';
        return (
          <li key={source.id} className={offline ? 'surface-muted flex min-w-0 flex-1 basis-72 items-center gap-3 border border-warning/30 px-4 py-2.5' : 'surface flex min-w-0 flex-1 basis-72 items-center gap-3 px-4 py-2.5'}>
            <span className={offline ? 'grid h-8 w-8 shrink-0 place-items-center rounded-field bg-warning/15 text-warning' : `grid h-8 w-8 shrink-0 place-items-center rounded-field ${MANGA.bgColor} ${MANGA.color}`}>
              {offline ? <CloudOff className="h-4 w-4" aria-hidden="true" /> : <Server className="h-4 w-4" aria-hidden="true" />}
            </span>
            <button type="button" className="min-w-0 flex-1 text-left" onClick={onManage} title={source.url}>
              <p className="truncate text-sm font-medium">{source.name}</p>
              <ServerStatus source={source} />
            </button>
            <span className="hidden shrink-0 text-xs tabular-nums text-base-content/55 sm:inline">{t('folders.series', { count: Number(source.series_count) })}</span>
            <Button appearance="ghost" size="sm" shape="square" disabled={running} aria-label={t('webdav.actions.sync', { name: source.name })} title={t('webdav.actions.sync', { name: source.name })} onClick={() => void syncSource(source.id)}>
              <RefreshCw className="h-4 w-4" />
            </Button>
          </li>
        );
      })}
    </ul>
  );
}
