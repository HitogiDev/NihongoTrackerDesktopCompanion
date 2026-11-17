import { useTranslation } from 'react-i18next';
import { Field, Toggle } from '../../components/ui/Field';
import { useAppStore } from '../../store/useAppStore';
import type { Settings } from '../../lib/types';

/** How the manga reader counts what is read. */
export function MangaSection({ settings }: { settings: Settings }) {
  const { t } = useTranslation(['settings', 'common']);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const dwell = Number(settings.manga_page_dwell_secs);
  const idleLog = Number(settings.manga_idle_log_minutes);

  return (
    <div className="surface p-2 divide-y divide-base-300/50">
      <Toggle
        label={t('manga.confirm')}
        description={t('manga.confirmHint')}
        checked={settings.manga_confirm_before_log}
        onChange={(v) => void updateSettings({ manga_confirm_before_log: v })}
      />
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
      <div className="p-3">
        <Field label={t('manga.idleLog')} hint={t('manga.idleLogHint')}>
          {(id) => (
            <input
              id={id}
              type="number"
              min={0}
              max={240}
              step={1}
              className="input w-32"
              value={idleLog}
              onChange={(e) => void updateSettings({ manga_idle_log_minutes: Math.max(0, Number(e.target.value)) })}
            />
          )}
        </Field>
      </div>
      <p className="p-3 text-xs text-base-content/60">{t('manga.clock')}</p>
    </div>
  );
}
