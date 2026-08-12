import { useEffect, useMemo, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
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
      title="Biblioteca"
      description="Ejecutables vinculados a una obra: al abrirlos se inicia la sesión y el texthooker."
      actions={
        <Button variant="primary" size="sm" onClick={() => setAdding(true)}>
          <Plus className="w-4 h-4" />
          Añadir
        </Button>
      }
    >
      <SectionTitle aside={<span className="text-xs text-base-content/50">{list.length}</span>}>
        Novelas y juegos
      </SectionTitle>

      {list.length === 0 ? (
        <div className="surface">
          <EmptyState
            icon={BookOpenText}
            title="Sin obras registradas"
            description="Vincula el .exe de una novela visual con su entrada en NihongoTracker para trackearla automáticamente."
            action={
              <Button variant="primary" size="sm" onClick={() => setAdding(true)}>
                <Plus className="w-4 h-4" />
                Añadir la primera
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
                      <span className="badge badge-ghost badge-xs mt-1">Desactivada</span>
                    ) : null}
                  </div>
                  <div className="flex flex-col gap-1">
                    <Button size="xs" appearance="outline" onClick={() => setEditing(app)}>
                      Editar
                    </Button>
                    <Button
                      size="xs"
                      appearance="ghost"
                      onClick={() => remove.mutate(app.exe)}
                      aria-label="Eliminar"
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
          <SectionTitle>Títulos vinculados a mano</SectionTitle>
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
    }));
    setSearching(false);
  }

  const valid = draft.exe.trim() && draft.media_id.trim() && draft.media_title.trim();

  return (
    <>
      <Modal
        open={open}
        onClose={onClose}
        title={initial ? 'Editar obra' : 'Añadir obra'}
        size="lg"
        actions={
          <>
            <Button appearance="ghost" onClick={onClose}>
              Cancelar
            </Button>
            <Button
              variant="primary"
              loading={saving}
              disabled={!valid}
              onClick={() => onSave({ ...draft, exe: draft.exe.trim().toLowerCase() })}
            >
              Guardar
            </Button>
          </>
        }
      >
        <div className="space-y-3">
          <Field label="Ejecutable" hint="El nombre del proceso, por ejemplo siglusengine.exe">
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
                  Elegir
                </Button>
              </div>
            )}
          </Field>

          <Field label="Obra en NihongoTracker">
            {() => (
              <div className="flex items-center gap-3 p-3 rounded-field bg-base-200/60">
                {draft.image ? (
                  <img src={draft.image} alt="" className="w-10 h-14 object-cover rounded" />
                ) : (
                  <div className="w-10 h-14 rounded bg-base-300" />
                )}
                <div className="min-w-0 flex-1">
                  <p className="font-medium truncate">
                    {draft.media_title || 'Sin seleccionar'}
                  </p>
                  <p className="text-xs text-base-content/60 font-mono">
                    {draft.media_id || '—'}
                  </p>
                </div>
                <Button appearance="outline" size="sm" onClick={() => setSearching(true)}>
                  Buscar
                </Button>
              </div>
            )}
          </Field>

          <Field
            label="URL extra del texthooker"
            hint="Opcional: se abrirá en el navegador junto al texthooker integrado."
          >
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
            label="Detección activa"
            description="Desactívala para dejar de trackear esta obra sin borrarla."
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
    <Modal open={open} onClose={onClose} title="Elegir proceso" size="lg">
      <div className="space-y-3">
        <div className="flex gap-2">
          <label className="input input-sm flex-1 flex items-center gap-2">
            <Search className="w-4 h-4 opacity-60" />
            <input
              className="grow"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              placeholder="Filtrar por nombre o título de ventana…"
            />
          </label>
          <Button size="sm" appearance="outline" loading={isFetching} onClick={() => void refetch()}>
            Actualizar
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
                {entry.title ? <span className="badge badge-xs badge-primary">ventana</span> : null}
              </button>
            </li>
          ))}
        </ul>
      </div>
    </Modal>
  );
}
