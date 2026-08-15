import { useEffect, useMemo, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { BookOpenText, Link2, Plus, Search, Trash2 } from 'lucide-react';
import { Button } from '../components/ui/Button';
import { EmptyState } from '../components/ui/Empty';
import { Field, Toggle } from '../components/ui/Field';
import { Modal } from '../components/ui/Modal';
import { Page, SectionTitle } from '../components/ui/Page';
import { MediaSearchModal } from '../components/MediaSearchModal';
import { getMediaTypeClasses } from '../constants/mediaColors';
import { ipc } from '../lib/ipc';
import type { MediaHit, ProcessEntry, VnApp } from '../lib/types';

export default function LibraryScreen() {
  const { t } = useTranslation(['library', 'common']);
  const queryClient = useQueryClient();
  const [adding, setAdding] = useState(false);
  const [editing, setEditing] = useState<VnApp | null>(null);

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
    onSuccess: (data) => queryClient.setQueryData(['vn-apps'], data),
  });

  const list = apps.data ?? [];

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
        <ul className="grid gap-3 md:grid-cols-2">
          {list.map((app) => {
            const type = getMediaTypeClasses(app.media_type);
            return (
              <li key={app.exe} className={`surface overflow-hidden border ${type.borderColor}`}>
                <div className={`h-1 w-full ${type.accentColor}`} />
                <div className="p-3 flex items-start gap-3">
                  {app.image ? (
                    <img src={app.image} alt="" className="w-12 h-16 object-cover rounded-field" />
                  ) : (
                    <div className="w-12 h-16 rounded-field bg-base-200 flex items-center justify-center">
                      <BookOpenText className="w-4 h-4 text-base-content/40" />
                    </div>
                  )}
                  <div className="min-w-0 flex-1">
                    <h3 className="font-semibold truncate">{app.media_title}</h3>
                    <p className="text-xs text-base-content/60 font-mono truncate">{app.exe}</p>
                    <p className="text-[11px] text-base-content/45 font-mono">{app.media_id}</p>
                    {!app.enabled ? (
                      <span className="badge badge-ghost badge-xs mt-1">{t('disabled')}</span>
                    ) : null}
                  </div>
                  <div className="flex flex-col gap-1">
                    <Button size="xs" appearance="outline" onClick={() => setEditing(app)}>
                      {t('common:edit')}
                    </Button>
                    <Button
                      size="xs"
                      appearance="ghost"
                      onClick={() => remove.mutate(app.exe)}
                      aria-label={t('common:delete')}
                    >
                      <Trash2 className="w-3 h-3" />
                    </Button>
                  </div>
                </div>
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
    </Page>
  );
}

const EMPTY: VnApp = {
  exe: '',
  media_id: '',
  media_type: 'vn',
  media_title: '',
  image: null,
  texthooker_url: null,
  enabled: true,
  is_adult: false,
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
    }));
    setSearching(false);
  }

  const valid = draft.exe.trim() && draft.media_id.trim() && draft.media_title.trim();

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
              onClick={() => onSave({ ...draft, exe: draft.exe.trim().toLowerCase() })}
            >
              {t('common:save')}
            </Button>
          </>
        }
      >
        <div className="space-y-3">
          <Field label={t('modal.executable')} hint={t('modal.executableHint')}>
            {(id) => (
              <div className="flex gap-2">
                <input
                  id={id}
                  className="input flex-1 font-mono text-sm"
                  value={draft.exe}
                  onChange={(e) => setDraft({ ...draft, exe: e.target.value })}
                  placeholder="mi-novela.exe"
                />
                <Button appearance="outline" onClick={() => setPicking(true)}>
                  <Search className="w-4 h-4" />
                  {t('modal.choose')}
                </Button>
              </div>
            )}
          </Field>

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

          <Field label={t('modal.extraUrl')} hint={t('modal.extraUrlHint')}>
            {(id) => (
              <input
                id={id}
                className="input w-full"
                value={draft.texthooker_url ?? ''}
                onChange={(e) =>
                  setDraft({ ...draft, texthooker_url: e.target.value || null })
                }
                placeholder="https://…"
              />
            )}
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
        initialQuery={draft.media_title}
        onPick={applyMedia}
      />
    </>
  );
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
