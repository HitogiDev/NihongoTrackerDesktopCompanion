import { useCallback, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { open } from '@tauri-apps/plugin-dialog';
import { ipc } from '../lib/ipc';
import { toast } from '../store/useToastStore';

export type DictionaryImportProgress = {
  current: number;
  total: number;
  file: string;
};

/**
 * Pick Yomitan zips and import them, reporting the outcome as toasts. Used by
 * Settings → Dictionaries and by the onboarding.
 */
export function useDictionaryImport() {
  const { t } = useTranslation('settings');
  const queryClient = useQueryClient();
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<DictionaryImportProgress | null>(null);
  const [imported, setImported] = useState(0);

  const importDictionary = useCallback(async () => {
    const selected = await open({
      multiple: true,
      directory: false,
      filters: [{ name: t('dictionary.fileFilter'), extensions: ['zip'] }],
    });
    const paths = selected === null ? [] : Array.isArray(selected) ? selected : [selected];
    if (paths.length === 0) return;
    setBusy(true);
    const errors: string[] = [];
    let done = 0;
    try {
      // One at a time: each import unpacks and indexes a whole zip, and running
      // them in parallel would just fight over the disk.
      for (const [index, path] of paths.entries()) {
        const file = path.split(/[\\/]/).pop() ?? path;
        setProgress({ current: index + 1, total: paths.length, file });
        try {
          await ipc.dictionaryImport(path);
          done += 1;
          await queryClient.invalidateQueries({ queryKey: ['dictionaries'] });
        } catch (error) {
          errors.push(paths.length > 1 ? `${file}: ${String(error)}` : String(error));
        }
      }
    } finally {
      setBusy(false);
      setProgress(null);
      setImported((count) => count + done);
      if (done > 0) {
        toast.success(done === 1 ? t('dictionary.imported') : t('dictionary.importedMany', { count: done }));
      }
      if (errors.length > 0) toast.error(errors.join('\n'));
    }
  }, [queryClient, t]);

  return { importDictionary, busy, progress, imported };
}
