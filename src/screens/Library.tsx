import { useEffect, useMemo, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { BookOpenText, FolderOpen, Grid2X2, Link2, List, Pencil, Play, Plus, Search, Trash2 } from 'lucide-react';
import { open as openFile } from '@tauri-apps/plugin-dialog';
import { Button } from '../components/ui/Button';
import { EmptyState } from '../components/ui/Empty';
import { Field, Toggle } from '../components/ui/Field';
import { Modal } from '../components/ui/Modal';
import { Page, SectionTitle } from '../components/ui/Page';
import { MediaSearchModal } from '../components/MediaSearchModal';
import { getMediaTypeClasses } from '../constants/mediaColors';
import { ipc } from '../lib/ipc';
import { useAppStore } from '../store/useAppStore';
import type { MediaHit, ProcessEntry, VnApp } from '../lib/types';

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
    onSuccess: (data) => {
      queryClient.setQueryData(['vn-apps'], data);
      setAdding(false);
      setEditing(null);
    },
  });

  const remove = useMutation({
    mutationFn: (exe: string) => ipc.deleteVnApp(exe),
    onSuccess: (data) => {
      queryClient.setQueryData(['vn-apps'], data);
      setDeleting(null);
    },
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
                            onClick={() => void ipc.launchProgram(app.launch_path!).catch(() => undefined)}
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
                        {app.hooker_path ? <Button size="xs" appearance="outline" onClick={() => void ipc.launchProgram(app.hooker_path!).catch(() => undefined)}>{t('launch.hooker')}</Button> : null}
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
                        {app.launch_path ? <Button size="xs" appearance="outline" onClick={() => void ipc.launchProgram(app.launch_path!).catch(() => undefined)}>{t('launch.game')}</Button> : null}
                        {app.hooker_path ? <Button size="xs" appearance="outline" onClick={() => void ipc.launchProgram(app.hooker_path!).catch(() => undefined)}>{t('launch.hooker')}</Button> : null}
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
                if (deleting) remove.mutate(deleting.exe);
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

const EMPTY: VnApp = {
  exe: '',
  media_id: '',
  media_type: 'vn',
  media_title: '',
  image: null,
  launch_path: null,
  hooker_path: null,
  native_hook_enabled: false,
  native_hook_code: null,
  reader_mode: 'integrated',
  enabled: true,
  is_adult: false,
  is_adult_image: null,
};

function VnAppModal({
  open,
  initial,
  onClose,
  onSave,
  saving,
}: {
  open: boolean;
  initial: VnApp | null;
  onClose: () => void;
  onSave: (entry: VnApp) => void;
  saving: boolean;
}) {
  const { t } = useTranslation(['library', 'common']);
  // Windows programs are .exe files; elsewhere any file (or a macOS .app) runs.
  const os = useAppStore((s) => s.tracker?.capabilities.os);
  const nativeHookAvailable = useAppStore((s) => s.tracker?.capabilities.native_hook ?? false);
  const programFilters = os === 'windows' ? [{ name: 'Programs', extensions: ['exe'] }] : undefined;
  const [draft, setDraft] = useState<VnApp>(EMPTY);
  const [picking, setPicking] = useState(false);
  const [searching, setSearching] = useState(false);

  useEffect(() => {
    if (open) setDraft(initial ?? EMPTY);
  }, [open, initial]);

  function applyMedia(hit: MediaHit, type: string) {
    setDraft((d) => ({
      ...d,
      media_id: hit.contentId,
      media_type: hit.type ?? type,
      media_title: hit.title.contentTitleRomaji || hit.title.contentTitleNative,
      image: hit.contentImage ?? hit.coverImage,
      // Carried along so Discord Rich Presence can strip the title later.
      is_adult: hit.isAdult,
      is_adult_image: hit.isAdultImage ?? null,
    }));
    setSearching(false);
  }

  const executableName = draft.exe.trim() || getExecutableName(draft.launch_path);
  const valid = executableName && draft.media_id.trim() && draft.media_title.trim();

  return (
    <>
      <Modal
        open={open}
        onClose={onClose}
        title={initial ? t('modal.editTitle') : t('modal.addTitle')}
        size="lg"
        actions={
          <>
            <Button appearance="ghost" onClick={onClose}>
              {t('common:cancel')}
            </Button>
            <Button
              variant="primary"
              loading={saving}
              disabled={!valid}
              onClick={() => onSave({ ...draft, exe: executableName.toLowerCase() })}
            >
              {t('common:save')}
            </Button>
          </>
        }
      >
        <div className="space-y-3">
          <Field label={t('modal.kind')}>
            {(id) => (
              <select
                id={id}
                className="select w-full"
                value={draft.media_type === 'game' ? 'game' : 'vn'}
                onChange={(e) => setDraft({ ...draft, media_type: e.target.value, media_id: '', media_title: '', image: null })}
              >
                <option value="vn">{t('common:logType.vn')}</option>
                <option value="game">{t('common:logType.game')}</option>
              </select>
            )}
          </Field>
          <Field label={t('modal.executable')} hint={t('modal.executableHint')}>
            {(id) => (
              <div className="flex gap-2">
                <input
                  id={id}
                  className="input flex-1 font-mono text-sm"
                  value={draft.exe}
                  onChange={(e) => setDraft({ ...draft, exe: e.target.value })}
                  placeholder={draft.launch_path ? getExecutableName(draft.launch_path) || t('modal.executablePlaceholder') : t('modal.executablePlaceholder')}
                />
                <Button appearance="outline" onClick={() => setPicking(true)}>
                  <Search className="w-4 h-4" />
                  {t('modal.choose')}
                </Button>
              </div>
            )}
          </Field>

          <Field label={t('modal.launchPath')} hint={t('modal.launchPathHint')}>
            {(id) => <div className="flex gap-2"><input id={id} className="input min-w-0 flex-1 font-mono text-sm" value={draft.launch_path ?? ''} onChange={(e) => setDraft({ ...draft, launch_path: e.target.value || null })} placeholder="C:\\Games\\game.exe" /><Button type="button" appearance="outline" aria-label={t('modal.choose')} onClick={async () => { const path = await openFile({ multiple: false, directory: false, filters: programFilters }); if (typeof path === 'string') setDraft({ ...draft, launch_path: path }); }}><FolderOpen className="h-4 w-4" /></Button></div>}
          </Field>
          <Field label={t('modal.hookerPath')} hint={t('modal.hookerPathHint')}>
            {(id) => <div className="flex gap-2"><input id={id} className="input min-w-0 flex-1 font-mono text-sm" value={draft.hooker_path ?? ''} onChange={(e) => setDraft({ ...draft, hooker_path: e.target.value || null })} placeholder="C:\\Tools\\Textractor.exe" /><Button type="button" appearance="outline" aria-label={t('modal.choose')} onClick={async () => { const path = await openFile({ multiple: false, directory: false, filters: [{ name: 'Hooker programs and scripts', extensions: ['exe', 'bat', 'cmd'] }] }); if (typeof path === 'string') setDraft({ ...draft, hooker_path: path }); }}><FolderOpen className="h-4 w-4" /></Button></div>}
          </Field>

          {nativeHookAvailable && (draft.media_type === 'game' || draft.media_type === 'vn') ? (
            <Toggle
              label={t('modal.nativeHook')}
              description={t('modal.nativeHookHint')}
              checked={draft.native_hook_enabled}
              disabled={draft.reader_mode === 'overlay'}
              onChange={(native_hook_enabled) => setDraft({ ...draft, native_hook_enabled })}
            />
          ) : null}

          <Field label={t('modal.work')}>
            {() => (
              <div className="flex items-center gap-3 p-3 rounded-field bg-base-200/60">
                {draft.image ? (
                  <img src={draft.image} alt="" className="w-10 h-14 object-cover rounded" />
                ) : (
                  <div className="w-10 h-14 rounded bg-base-300" />
                )}
                <div className="min-w-0 flex-1">
                  <p className="font-medium truncate">
                    {draft.media_title || t('modal.notSelected')}
                  </p>
                  <p className="text-xs text-base-content/60 font-mono">
                    {draft.media_id || '—'}
                  </p>
                </div>
                <Button appearance="outline" size="sm" onClick={() => setSearching(true)}>
                  {t('common:search')}
                </Button>
              </div>
            )}
          </Field>

          <Field label={t('modal.readerMode')} hint={draft.reader_mode === 'overlay' ? t('modal.overlayHint') : undefined}>
            <div className="join flex flex-wrap" role="group" aria-label={t('modal.readerMode')}>
              {(['integrated', 'browser', 'overlay'] as const).filter((mode) => mode !== 'overlay' || draft.media_type === 'vn').map((mode) => (
                <button key={mode} type="button" aria-pressed={draft.reader_mode === mode}
                  className={draft.reader_mode === mode ? 'join-item btn btn-primary btn-sm' : 'join-item btn btn-outline btn-sm'}
                  onClick={() => setDraft({ ...draft, reader_mode: mode, native_hook_enabled: mode === 'overlay' || draft.native_hook_enabled })}>
                  {t(`modal.readerModes.${mode}`)}
                </button>
              ))}
            </div>
          </Field>

          <Toggle
            label={t('modal.enabled')}
            description={t('modal.enabledHint')}
            checked={draft.enabled}
            onChange={(enabled) => setDraft({ ...draft, enabled })}
          />
        </div>
      </Modal>

      <ProcessPickerModal
        open={picking}
        onClose={() => setPicking(false)}
        onPick={(entry) => {
          setDraft((d) => ({ ...d, exe: entry.name }));
          setPicking(false);
        }}
      />

      <MediaSearchModal
        open={searching}
        onClose={() => setSearching(false)}
        mediaType={draft.media_type || 'vn'}
        allowTypeChange={false}
        initialQuery={draft.media_title}
        onPick={applyMedia}
      />
    </>
  );
}

function getExecutableName(path: string | null) {
  const parts = path?.trim().split(/[\\/]/).filter(Boolean) ?? [];
  // A macOS bundle `Novel.app` runs as the process `Novel`.
  return (parts[parts.length - 1] ?? '').replace(/\.app$/i, '');
}

function ProcessPickerModal({
  open,
  onClose,
  onPick,
}: {
  open: boolean;
  onClose: () => void;
  onPick: (entry: ProcessEntry) => void;
}) {
  const { t } = useTranslation(['library', 'common']);
  const [filter, setFilter] = useState('');
  const { data, isFetching, refetch } = useQuery({
    queryKey: ['processes'],
    queryFn: ipc.listProcesses,
    enabled: open,
  });

  const filtered = useMemo(() => {
    const q = filter.trim().toLowerCase();
    const list = data ?? [];
    if (!q) return list;
    return list.filter(
      (p) => p.name.includes(q) || (p.title ?? '').toLowerCase().includes(q)
    );
  }, [data, filter]);

  return (
    <Modal open={open} onClose={onClose} title={t('processes.title')} size="lg">
      <div className="space-y-3">
        <div className="flex gap-2">
          <label className="input input-sm flex-1 flex items-center gap-2">
            <Search className="w-4 h-4 opacity-60" />
            <input
              className="grow"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              placeholder={t('processes.filter')}
            />
          </label>
          <Button size="sm" appearance="outline" loading={isFetching} onClick={() => void refetch()}>
            {t('common:refresh')}
          </Button>
        </div>
        <ul className="max-h-96 overflow-y-auto space-y-1">
          {filtered.map((entry) => (
            <li key={`${entry.name}-${entry.pid}`}>
              <button
                type="button"
                className="flex w-full items-center gap-3 rounded-field px-3 py-2 text-left transition-colors hover:bg-base-200 cursor-pointer"
                onClick={() => onPick(entry)}
              >
                <span className="min-w-0 flex-1">
                  <span className="block font-mono text-sm truncate">{entry.name}</span>
                  {entry.title ? (
                    <span className="block text-xs text-base-content/60 truncate">
                      {entry.title}
                    </span>
                  ) : null}
                </span>
                {entry.title ? (
                  <span className="badge badge-xs badge-primary">{t('processes.window')}</span>
                ) : null}
              </button>
            </li>
          ))}
        </ul>
      </div>
    </Modal>
  );
}
