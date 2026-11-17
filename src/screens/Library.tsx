import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { BookOpenText, Grid2X2, Link2, List, Pencil, Play, Plus, Search, Trash2 } from 'lucide-react';
import { Button } from '../components/ui/Button';
import { EmptyState } from '../components/ui/Empty';
import { Modal } from '../components/ui/Modal';
import { Page, SectionTitle } from '../components/ui/Page';
import { VnAppModal } from '../components/VnAppModal';
import { getMediaTypeClasses } from '../constants/mediaColors';
import { ipc } from '../lib/ipc';
import { toast } from '../store/useToastStore';
import i18n from '../i18n';
import type { VnApp } from '../lib/types';

export default function LibraryScreen() {
  const { t } = useTranslation(['library', 'common']);
  const queryClient = useQueryClient();
  const [adding, setAdding] = useState(false);
  const [editing, setEditing] = useState<VnApp | null>(null);
  const [deleting, setDeleting] = useState<VnApp | null>(null);
  const [view, setView] = useState<'grid' | 'list'>('grid');
  const [mediaFilter, setMediaFilter] = useState<'all' | 'vn' | 'game'>('all');
  const [search, setSearch] = useState('');

  const apps = useQuery({ queryKey: ['vn-apps'], queryFn: ipc.listVnApps });
  const mappings = useQuery({ queryKey: ['title-maps'], queryFn: ipc.listTitleMappings });

  const save = useMutation({
    mutationFn: (entry: VnApp) => ipc.upsertVnApp(entry),
    onSuccess: (data, entry) => {
      queryClient.setQueryData(['vn-apps'], data);
      setAdding(false);
      setEditing(null);
      toast.success(t('toast.saved', { title: entry.media_title }));
    },
    onError: (error) => toast.error(t('toast.saveFailed', { message: String(error) })),
  });

  const remove = useMutation({
    mutationFn: (app: VnApp) => ipc.deleteVnApp(app.exe),
    onSuccess: (data, app) => {
      queryClient.setQueryData(['vn-apps'], data);
      setDeleting(null);
      toast.success(t('toast.removed', { title: app.media_title }));
    },
    onError: (error) => toast.error(t('toast.removeFailed', { message: String(error) })),
  });

  const list = (apps.data ?? []).filter((app) => (mediaFilter === 'all' || app.media_type === mediaFilter) && `${app.media_title} ${app.exe}`.toLowerCase().includes(search.toLowerCase()));

  return (
    <Page
      title={t('title')}
      description={t('subtitle')}
      actions={
        <Button variant="primary" size="sm" onClick={() => setAdding(true)}>
          <Plus className="w-4 h-4" />
          {t('common:add')}
        </Button>
      }
    >
      <div className="flex flex-wrap items-center gap-2">
        <label className="input input-sm min-w-40 flex-1"><Search className="h-4 w-4 opacity-60" /><input value={search} onChange={(event) => setSearch(event.target.value)} placeholder={t('filter.search')} /></label>
        <select className="select select-sm" value={mediaFilter} onChange={(event) => setMediaFilter(event.target.value as typeof mediaFilter)}><option value="all">{t('filter.all')}</option><option value="vn">{t('common:logType.vn')}</option><option value="game">{t('common:logType.game')}</option></select>
        <div className="join"><button type="button" className={`join-item btn btn-sm ${view === 'grid' ? 'btn-active' : ''}`} aria-label={t('view.grid')} onClick={() => setView('grid')}><Grid2X2 className="h-4 w-4" /></button><button type="button" className={`join-item btn btn-sm ${view === 'list' ? 'btn-active' : ''}`} aria-label={t('view.list')} onClick={() => setView('list')}><List className="h-4 w-4" /></button></div>
      </div>
      <SectionTitle aside={<span className="text-xs text-base-content/50">{list.length}</span>}>
        {t('works')}
      </SectionTitle>

      {list.length === 0 ? (
        <div className="surface">
          <EmptyState
            icon={BookOpenText}
            title={t('empty')}
            description={t('emptyBody')}
            action={
              <Button variant="primary" size="sm" onClick={() => setAdding(true)}>
                <Plus className="w-4 h-4" />
                {t('addFirst')}
              </Button>
            }
          />
        </div>
      ) : (
        <ul className={view === 'grid' ? 'grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5' : 'space-y-2'}>
          {list.map((app) => {
            const type = getMediaTypeClasses(app.media_type);
            return (
              <li key={app.exe} className={`surface overflow-hidden border ${type.borderColor}`}>
                {view === 'grid' ? (
                  <>
                    <div className={`h-1 w-full ${type.accentColor}`} />
                    <div className="group/image relative aspect-[3/4] w-full overflow-hidden bg-base-200">
                      {app.image ? (
                        <img src={app.image} alt={app.media_title} className="h-full w-full object-cover" />
                      ) : (
                        <div className="flex h-full w-full items-center justify-center">
                          <BookOpenText className="h-8 w-8 text-base-content/40" />
                        </div>
                      )}
                      <Button
                        size="sm"
                        variant="error"
                        appearance="ghost"
                        shape="square"
                        className="absolute left-2 top-2 z-10 bg-base-100/90 shadow-sm opacity-100 transition-opacity sm:opacity-0 sm:group-hover/image:opacity-100 sm:group-focus-within/image:opacity-100"
                        aria-label={t('actions.remove', { title: app.media_title })}
                        title={t('actions.remove', { title: app.media_title })}
                        onClick={() => setDeleting(app)}
                      >
                        <Trash2 className="h-4 w-4" />
                      </Button>
                      <Button
                        size="sm"
                        appearance="ghost"
                        shape="square"
                        className="absolute right-2 top-2 z-10 bg-base-100/90 shadow-sm opacity-100 transition-opacity sm:opacity-0 sm:group-hover/image:opacity-100 sm:group-focus-within/image:opacity-100"
                        aria-label={t('actions.edit', { title: app.media_title })}
                        title={t('actions.edit', { title: app.media_title })}
                        onClick={() => setEditing(app)}
                      >
                        <Pencil className="h-4 w-4" />
                      </Button>
                      {app.launch_path ? (
                        <div className="absolute inset-0 flex items-center justify-center bg-neutral/45 opacity-100 transition-opacity sm:bg-neutral/45 sm:opacity-0 sm:group-hover/image:opacity-100 sm:group-focus-within/image:opacity-100">
                          <Button
                            size="sm"
                            variant="primary"
                            onClick={() => void ipc.launchProgram(app.launch_path!).catch(launchFailed)}
                          >
                            <Play className="h-4 w-4" />
                            {t('launch.clickToRun')}
                          </Button>
                        </div>
                      ) : null}
                    </div>
                    <div className="space-y-2 p-3">
                      <div className="min-w-0">
                        <h3 className="truncate font-semibold" title={app.media_title}>{app.media_title}</h3>
                        <p className="truncate font-mono text-xs text-base-content/60" title={app.exe}>{app.exe}</p>
                        {!app.enabled ? <span className="badge badge-ghost badge-xs mt-1">{t('disabled')}</span> : null}
                      </div>
                      <div className="flex flex-wrap items-center gap-1">
                        {app.hooker_path ? <Button size="xs" appearance="outline" onClick={() => void ipc.launchProgram(app.hooker_path!).catch(launchFailed)}>{t('launch.hooker')}</Button> : null}
                      </div>
                    </div>
                  </>
                ) : (
                  <div className="flex items-center">
                    <div className={`h-full w-1 self-stretch ${type.accentColor}`} />
                    <div className="flex min-w-0 flex-1 items-start gap-3 p-3">
                      {app.image ? (
                        <img src={app.image} alt="" className="h-16 w-12 rounded-field object-cover" />
                      ) : (
                        <div className="flex h-16 w-12 items-center justify-center rounded-field bg-base-200">
                          <BookOpenText className="h-4 w-4 text-base-content/40" />
                        </div>
                      )}
                      <div className="min-w-0 flex-1">
                        <h3 className="truncate font-semibold">{app.media_title}</h3>
                        <p className="truncate font-mono text-xs text-base-content/60">{app.exe}</p>
                        <p className="font-mono text-[11px] text-base-content/45">{app.media_id}</p>
                        {!app.enabled ? <span className="badge badge-ghost badge-xs mt-1">{t('disabled')}</span> : null}
                      </div>
                      <div className="flex flex-col gap-1">
                        {app.launch_path ? <Button size="xs" appearance="outline" onClick={() => void ipc.launchProgram(app.launch_path!).catch(launchFailed)}>{t('launch.game')}</Button> : null}
                        {app.hooker_path ? <Button size="xs" appearance="outline" onClick={() => void ipc.launchProgram(app.hooker_path!).catch(launchFailed)}>{t('launch.hooker')}</Button> : null}
                        <Button size="xs" appearance="outline" onClick={() => setEditing(app)}>{t('common:edit')}</Button>
                        <Button size="xs" appearance="ghost" onClick={() => setDeleting(app)} aria-label={t('actions.remove', { title: app.media_title })}>
                          <Trash2 className="h-3 w-3" />
                        </Button>
                      </div>
                    </div>
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      )}

      {(mappings.data ?? []).length > 0 ? (
        <>
          <SectionTitle>{t('manualMappings')}</SectionTitle>
          <ul className="space-y-1.5">
            {(mappings.data ?? []).map(({ title, media }) => (
              <li
                key={title}
                className="flex items-center gap-3 px-3 py-2 rounded-field bg-base-100/60"
              >
                <Link2 className="w-4 h-4 text-base-content/40 shrink-0" />
                <span className="text-sm truncate flex-1">{title}</span>
                <span className="text-xs text-base-content/60 truncate max-w-[40%]">
                  {media?.title}
                </span>
                <Button
                  size="xs"
                  appearance="ghost"
                  onClick={() =>
                    void ipc.deleteTitleMapping(title).then(() => mappings.refetch())
                  }
                >
                  <Trash2 className="w-3 h-3" />
                </Button>
              </li>
            ))}
          </ul>
        </>
      ) : null}

      <VnAppModal
        open={adding || Boolean(editing)}
        initial={editing}
        onClose={() => {
          setAdding(false);
          setEditing(null);
        }}
        onSave={(entry) => save.mutate(entry)}
        saving={save.isPending}
      />
      <Modal
        open={Boolean(deleting)}
        onClose={() => setDeleting(null)}
        title={t('actions.confirmTitle')}
        actions={
          <>
            <Button appearance="ghost" disabled={remove.isPending} onClick={() => setDeleting(null)}>
              {t('common:cancel')}
            </Button>
            <Button
              variant="error"
              loading={remove.isPending}
              onClick={() => {
                if (deleting) remove.mutate(deleting);
              }}
            >
              <Trash2 className="h-4 w-4" />
              {t('common:delete')}
            </Button>
          </>
        }
      >
        <p>{t('actions.confirmBody', { title: deleting?.media_title ?? '' })}</p>
      </Modal>
    </Page>
  );
}

function launchFailed(error: unknown) {
  toast.error(i18n.t('library:toast.launchFailed', { message: String(error) }));
}
