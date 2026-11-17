import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { BookOpen, Trash2 } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { ipc } from '../../lib/ipc';
import { useDictionaryImport } from '../../hooks/useDictionaryImport';
import { toast } from '../../store/useToastStore';

export function DictionariesSection() {
  const { t } = useTranslation(['settings', 'common']);
  const queryClient = useQueryClient();
  const dictionaries = useQuery({ queryKey: ['dictionaries'], queryFn: ipc.dictionaryList });
  const { importDictionary, busy: dictionaryBusy, progress: dictionaryProgress } = useDictionaryImport();

  return (
    <>
      <div className="surface p-4 space-y-3">
        <p className="text-sm text-base-content/70">{t('dictionary.hint')}</p>
        <Button size="sm" variant="primary" loading={dictionaryBusy} onClick={() => void importDictionary()}>
          <BookOpen className="w-4 h-4" />
          {t('dictionary.import')}
        </Button>
        {dictionaryProgress ? (
          <p className="text-sm text-base-content/70 truncate">
            {t('dictionary.progress', dictionaryProgress)}
          </p>
        ) : null}
        {(dictionaries.data ?? []).length > 0 ? (
          <ul className="space-y-1.5">
            {(dictionaries.data ?? []).map((dictionary) => (
              <li key={dictionary.id} className="flex items-center gap-3 rounded-field bg-base-200/60 px-3 py-2">
                <BookOpen className="w-4 h-4 text-base-content/50 shrink-0" />
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-sm font-medium">{dictionary.title}</span>
                  <span className="block text-xs text-base-content/50">{dictionary.termCount.toLocaleString()} {t('dictionary.entries')}</span>
                </span>
                <Button
                  size="xs"
                  appearance="ghost"
                  aria-label={t('dictionary.remove')}
                  onClick={() =>
                    void ipc.dictionaryRemove(dictionary.id)
                      .then(() => {
                        toast.success(t('dictionary.removed'));
                        return queryClient.invalidateQueries({ queryKey: ['dictionaries'] });
                      })
                      .catch((error) => toast.error(String(error)))
                  }
                >
                  <Trash2 className="w-4 h-4" />
                </Button>
              </li>
            ))}
          </ul>
        ) : (
          <p className="text-xs text-base-content/50">{t('dictionary.none')}</p>
        )}
      </div>
    </>
  );
}
