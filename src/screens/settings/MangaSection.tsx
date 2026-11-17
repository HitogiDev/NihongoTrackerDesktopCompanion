import { useTranslation } from 'react-i18next';
import { Field } from '../../components/ui/Field';
import { useAppStore } from '../../store/useAppStore';
import type { Settings } from '../../lib/types';

/** How the manga reader counts what is read. */
export function MangaSection({ settings }: { settings: Settings }) {
  const { t } = useTranslation(['settings', 'common']);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const dwell = Number(settings.manga_page_dwell_secs);

  return (
    <div className="surface p-2 divide-y divide-base-300/50">
      <div className="p-3">
        <Field label={t('manga.dwell', { count: dwell })} hint={t('manga.dwellHint')}>
          {(id) => (
            <input
              id={id}
              type="range"
              min={1}
              max={15}
              step={1}
              className="range range-primary range-sm"
              value={dwell}
              onChange={(e) => void updateSettings({ manga_page_dwell_secs: Number(e.target.value) })}
            />
          )}
        </Field>
      </div>
    </div>
  );
}
