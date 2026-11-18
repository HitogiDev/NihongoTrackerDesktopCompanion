import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { AlertTriangle, FileDown, Server, UploadCloud } from 'lucide-react';
import { Button } from '../../../components/ui/Button';
import { Modal } from '../../../components/ui/Modal';
import { ipc } from '../../../lib/ipc';
import { displayUrl } from '../../../lib/manga';
import type { MangaSource, MangaVolume } from '../../../lib/types';
import { toast } from '../../../store/useToastStore';
import { ocrErrorText } from './useOcr';


/**
 * Exporting a volume's text as a `.mokuro`: written beside the volume on
 * disk, or, for a WebDAV volume, uploaded beside it on the server, which
 * only happens once the user confirms here. A file already there is only
 * replaced after a second, explicit confirmation.
 */
export function OcrExportModal({ open, onClose, volume, source }: {
  open: boolean;
  onClose: () => void;
  volume: MangaVolume;
  source: MangaSource | null;
}) {
  const { t } = useTranslation(['manga', 'common']);
  const remote = volume.format === 'remote-archive' || volume.format === 'remote-folder';
  const [busy, setBusy] = useState(false);
  const [exists, setExists] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const close = () => {
    setExists(false);
    setError(null);
    onClose();
  };
  const run = async (overwrite: boolean) => {
    setBusy(true);
    setError(null);
    try {
      const done = await ipc.mangaOcrExport(volume.id, overwrite);
      toast.success(t(done.remote ? 'ocr.export.uploaded' : 'ocr.export.saved', { path: done.path, count: Number(done.pages_with_text) }));
      close();
    } catch (failure) {
      if (String(failure) === 'exists') setExists(true);
      else setError(String(failure));
    } finally {
      setBusy(false);
    }
  };

  const Icon = remote ? UploadCloud : FileDown;
  return (
    <Modal
      open={open}
      onClose={close}
      title={t(remote ? 'ocr.export.titleRemote' : 'ocr.export.title')}
      actions={(
        <>
          <Button appearance="ghost" onClick={close}>{t('common:cancel')}</Button>
          {exists ? (
            <Button variant="warning" loading={busy} onClick={() => void run(true)}>
              {busy ? null : <Icon className="h-4 w-4" />}
              {t('ocr.export.replace')}
            </Button>
          ) : (
            <Button variant="primary" loading={busy} onClick={() => void run(false)}>
              {busy ? null : <Icon className="h-4 w-4" />}
              {t(remote ? 'ocr.export.upload' : 'ocr.export.save')}
            </Button>
          )}
        </>
      )}
    >
      <div className="space-y-4 text-sm">
        <p className="text-base-content/75">{t(remote ? 'ocr.export.bodyRemote' : 'ocr.export.body')}</p>
        <div className="surface-muted flex items-center gap-3 p-3">
          <span className="grid h-9 w-9 shrink-0 place-items-center rounded-field bg-base-100 shadow-sm">
            {remote ? <Server className="h-4 w-4" aria-hidden="true" /> : <FileDown className="h-4 w-4" aria-hidden="true" />}
          </span>
          <div className="min-w-0">
            <p className="truncate font-mono text-xs">{`${volume.title}.mokuro`}</p>
            <p className="truncate text-xs text-base-content/55">
              {remote && source ? `${source.name} · ${displayUrl(source.url)}` : t('ocr.export.besideVolume', { title: volume.title })}
            </p>
          </div>
        </div>
        {remote ? <p className="text-xs text-base-content/60">{t('ocr.export.permission')}</p> : null}
        {exists ? (
          <div role="alert" className="alert alert-warning alert-soft">
            <AlertTriangle className="h-4 w-4" aria-hidden="true" />
            <span>{t(remote ? 'ocr.export.existsRemote' : 'ocr.export.exists')}</span>
          </div>
        ) : null}
        {error ? (
          <div role="alert" className="alert alert-error alert-soft">
            <AlertTriangle className="h-4 w-4" aria-hidden="true" />
            <span>{ocrErrorText(error)}</span>
          </div>
        ) : null}
      </div>
    </Modal>
  );
}
