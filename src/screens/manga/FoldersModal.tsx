import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Folder, FolderPlus, Trash2 } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Modal } from '../../components/ui/Modal';
import { ipc } from '../../lib/ipc';
import type { MangaSource } from '../../lib/types';
import { toast } from '../../store/useToastStore';

/** The library's roots: what is scanned, and a way to forget one. */
export function FoldersModal({ open, onClose, sources, onAdd, adding }: {
  open: boolean;
  onClose: () => void;
  sources: MangaSource[];
  onAdd: () => void;
  adding: boolean;
}) {
  const { t } = useTranslation(['manga', 'common']);
  const [removing, setRemoving] = useState<string | null>(null);
  const local = sources.filter((source) => source.kind === 'local');

  const remove = async (source: MangaSource) => {
    setRemoving(source.id);
    try {
      await ipc.mangaRemoveSource(source.id);
      toast.success(t('toast.removed', { name: source.name }));
    } catch (error) {
      toast.error(t('toast.removeFailed', { message: String(error) }));
    } finally {
      setRemoving(null);
    }
  };

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={t('folders.title')}
      size="lg"
      actions={
        <Button variant="primary" size="sm" loading={adding} onClick={onAdd}>
          {adding ? null : <FolderPlus className="h-4 w-4" />}
          {t('actions.addFolder')}
        </Button>
      }
    >
      <p className="-mt-2 mb-4 text-sm text-base-content/60">{t('folders.body')}</p>
      {local.length === 0 ? (
        <p className="surface-muted px-4 py-6 text-center text-sm text-base-content/60">{t('folders.none')}</p>
      ) : (
        <ul className="surface-muted divide-y divide-base-300/60">
          {local.map((source) => (
            <li key={source.id} className="flex items-center gap-3 px-4 py-3">
              <Folder className="h-4 w-4 shrink-0 text-base-content/50" aria-hidden="true" />
              <div className="min-w-0 flex-1">
                <p className="truncate text-sm font-medium">{source.name}</p>
                <p className="selectable truncate font-mono text-[11px] text-base-content/50" title={source.url}>{source.url}</p>
              </div>
              <span className="shrink-0 text-xs text-base-content/50">{t('folders.series', { count: Number(source.series_count) })}</span>
              <Button
                appearance="ghost"
                size="sm"
                shape="square"
                loading={removing === source.id}
                aria-label={t('folders.remove', { name: source.name })}
                title={t('folders.remove', { name: source.name })}
                onClick={() => void remove(source)}
              >
                {removing === source.id ? null : <Trash2 className="h-4 w-4" />}
              </Button>
            </li>
          ))}
        </ul>
      )}
      <p className="mt-3 text-xs text-base-content/50">{t('folders.removeHint')}</p>
    </Modal>
  );
}
