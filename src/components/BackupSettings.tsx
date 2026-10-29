import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { open as openFile, save as saveFile } from '@tauri-apps/plugin-dialog';
import { DatabaseBackup, Upload } from 'lucide-react';
import { Button } from './ui/Button';
import { Modal } from './ui/Modal';
import { ipc } from '../lib/ipc';

const FILTERS = [{ name: 'SQLite', extensions: ['sqlite3', 'sqlite', 'db'] }];

/**
 * Settings → App: copy the local database out, or replace it with a copy. A
 * restore restarts the app, so nothing keeps running on the old data.
 */
export function BackupSettings() {
  const { t } = useTranslation(['settings', 'common']);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const [restoring, setRestoring] = useState<string | null>(null);

  const exportBackup = async () => {
    const day = new Date().toISOString().slice(0, 10);
    const path = await saveFile({ defaultPath: `nihongotracker-companion-${day}.sqlite3`, filters: FILTERS });
    if (!path) return;
    setBusy(true);
    setMessage(null);
    try {
      await ipc.exportDatabase(path);
      setMessage({ ok: true, text: t('backup.exported', { path }) });
    } catch (cause) {
      setMessage({ ok: false, text: String(cause) });
    } finally {
      setBusy(false);
    }
  };

  const pickRestore = async () => {
    const path = await openFile({ multiple: false, directory: false, filters: FILTERS });
    if (typeof path === 'string') setRestoring(path);
  };

  const restore = async () => {
    if (!restoring) return;
    setBusy(true);
    setMessage(null);
    try {
      // Resolves only when the restore fails: on success the app restarts.
      await ipc.restoreDatabase(restoring);
    } catch (cause) {
      setMessage({ ok: false, text: String(cause) });
      setRestoring(null);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="surface p-2">
      <div className="flex flex-wrap items-center gap-3 p-3">
        <div className="min-w-0 flex-1">
          <p className="font-medium">{t('backup.title')}</p>
          <p className="text-xs text-base-content/60">{t('backup.hint')}</p>
        </div>
        <Button size="sm" appearance="outline" disabled={busy} onClick={() => void exportBackup()}>
          <DatabaseBackup className="h-4 w-4" />
          {t('backup.export')}
        </Button>
        <Button size="sm" appearance="outline" disabled={busy} onClick={() => void pickRestore()}>
          <Upload className="h-4 w-4" />
          {t('backup.restore')}
        </Button>
      </div>
      {message ? (
        <p className={`px-3 pb-3 text-xs break-all ${message.ok ? 'text-success' : 'text-error'}`}>{message.text}</p>
      ) : null}

      <Modal
        open={restoring !== null}
        onClose={() => !busy && setRestoring(null)}
        title={t('backup.confirmTitle')}
        actions={
          <>
            <Button appearance="ghost" disabled={busy} onClick={() => setRestoring(null)}>
              {t('common:cancel')}
            </Button>
            <Button variant="error" loading={busy} onClick={() => void restore()}>
              {t('backup.confirm')}
            </Button>
          </>
        }
      >
        <p className="text-sm">{t('backup.confirmBody')}</p>
        <p className="mt-2 text-xs font-mono break-all text-base-content/60">{restoring}</p>
      </Modal>
    </div>
  );
}
