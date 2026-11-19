import { useTranslation } from 'react-i18next';
import { NativeHookTextFilters } from '../../components/NativeHookTextFilters';
import { Field, Toggle } from '../../components/ui/Field';
import { SectionTitle } from '../../components/ui/Page';
import { useAppStore } from '../../store/useAppStore';
import type { Settings } from '../../lib/types';

/** Visual novels, games and the text that comes out of them. */
export function NovelsSection({ settings }: { settings: Settings }) {
  const { t } = useTranslation(['settings', 'common']);
  const updateSettings = useAppStore((s) => s.updateSettings);

  return (
    <>
      <SectionTitle>{t('vn.title')}</SectionTitle>
      <div className="surface p-2 divide-y divide-base-300/50">
        <Toggle
          label={t('vn.detect')}
          checked={settings.detect_visual_novels}
          onChange={(v) => void updateSettings({ detect_visual_novels: v })}
        />
        <Toggle
          label={t('vn.requireFocus')}
          checked={settings.vn_require_focus}
          onChange={(v) => void updateSettings({ vn_require_focus: v })}
        />
        <Toggle
          label={t('vn.openTexthooker')}
          checked={settings.vn_open_texthooker}
          onChange={(v) => void updateSettings({ vn_open_texthooker: v })}
        />
        <Toggle
          label={t('vn.confirm')}
          checked={settings.vn_confirm_before_log}
          onChange={(v) => void updateSettings({ vn_confirm_before_log: v })}
        />
        <div className="p-3">
          <Field
            label={t('vn.afk')}
            hint={t('vn.afkHint')}
          >
            {(id) => (
              <input
                id={id}
                type="number"
                min={0}
                max={600}
                step={1}
                className="input w-32"
                value={settings.afk_timeout_secs}
                onChange={(e) => void updateSettings({ afk_timeout_secs: Number(e.target.value) })}
              />
            )}
          </Field>
        </div>
      </div>

      <SectionTitle>{t('game.title')}</SectionTitle>
      <div className="surface p-2 divide-y divide-base-300/50">
        <Toggle
          label={t('game.detect')}
          checked={settings.detect_games}
          onChange={(v) => void updateSettings({ detect_games: v })}
        />
        <p className="p-3 text-sm text-base-content/60">{t('game.rules')}</p>
      </div>

      <NativeHookTextFilters />
    </>
  );
}
