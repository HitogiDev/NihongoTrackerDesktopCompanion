import { useTranslation } from 'react-i18next';
import { PlatformSupport } from '../../components/PlatformSupport';
import { Field } from '../../components/ui/Field';
import { SectionTitle } from '../../components/ui/Page';
import { useAppStore } from '../../store/useAppStore';
import type { Settings } from '../../lib/types';

export function AdvancedSection({ settings }: { settings: Settings }) {
  const { t } = useTranslation(['settings', 'common']);
  const updateSettings = useAppStore((s) => s.updateSettings);

  return (
    <>
      <SectionTitle>{t('platform.title')}</SectionTitle>
      <PlatformSupport />
      <SectionTitle>{t('sections.detectionLoop')}</SectionTitle>
      <div className="surface p-2">
        <div className="p-3">
          <Field
            label={t('app.poll', { ms: settings.poll_interval_ms })}
            hint={t('app.pollHint')}
          >
            {(id) => (
              <input
                id={id}
                type="range"
                min={500}
                max={5000}
                step={250}
                className="range range-primary"
                value={settings.poll_interval_ms}
                onChange={(e) => void updateSettings({ poll_interval_ms: Number(e.target.value) })}
              />
            )}
          </Field>
        </div>
      </div>
    </>
  );
}
