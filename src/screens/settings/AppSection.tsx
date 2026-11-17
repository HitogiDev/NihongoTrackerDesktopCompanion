import { useTranslation } from 'react-i18next';
import { Field, Toggle } from '../../components/ui/Field';
import { SectionTitle } from '../../components/ui/Page';
import {
  LANGUAGE_LABELS,
  SUPPORTED_LANGUAGES,
  type SupportedLanguage,
} from '../../i18n/languages';
import { useAppStore } from '../../store/useAppStore';
import type { Settings } from '../../lib/types';

// Same list daisyUI is compiled with in index.css, mirroring the web app.
const THEMES = [
  'system',
  'dark',
  'light',
  'cupcake',
  'bumblebee',
  'emerald',
  'corporate',
  'synthwave',
  'retro',
  'valentine',
  'halloween',
  'garden',
  'forest',
  'dracula',
  'cmyk',
  'night',
  'winter',
  'dim',
  'sunset',
  'abyss',
];

/** Appearance, language, start-up, notifications and sharing. */
export function AppSection({ settings }: { settings: Settings }) {
  const { t, i18n } = useTranslation(['settings', 'common']);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const currentLanguage = settings.language || i18n.language;

  return (
    <>
      <div className="surface p-2 divide-y divide-base-300/50">
        <Toggle
          label={t('app.overlay')}
          description={t('app.overlayHint')}
          checked={settings.overlay_enabled}
          onChange={(v) => void updateSettings({ overlay_enabled: v })}
        />
        <div className="p-3">
          <Field label={t('app.language')}>
            {(id) => (
              <select
                id={id}
                className="select w-full"
                value={
                  SUPPORTED_LANGUAGES.includes(currentLanguage as SupportedLanguage)
                    ? currentLanguage
                    : 'en'
                }
                onChange={(e) => void updateSettings({ language: e.target.value })}
              >
                {SUPPORTED_LANGUAGES.map((code) => (
                  <option key={code} value={code}>
                    {LANGUAGE_LABELS[code]}
                  </option>
                ))}
              </select>
            )}
          </Field>
        </div>
        <Toggle
          label={t('app.autostart')}
          checked={settings.autostart}
          onChange={(v) => void updateSettings({ autostart: v })}
        />
        <Toggle
          label={t('app.startMinimized')}
          checked={settings.start_minimized}
          onChange={(v) => void updateSettings({ start_minimized: v })}
        />
        <Toggle
          label={t('app.minimizeToTray')}
          description={t('app.minimizeToTrayHint')}
          checked={settings.minimize_to_tray}
          onChange={(v) => void updateSettings({ minimize_to_tray: v })}
        />
        <Toggle
          label={t('app.notifications')}
          checked={settings.notifications}
          onChange={(v) => void updateSettings({ notifications: v })}
        />
        <Toggle
          label={t('app.streakReminder')}
          description={t('app.streakReminderHint')}
          checked={settings.streak_reminder}
          disabled={!settings.notifications}
          onChange={(v) => void updateSettings({ streak_reminder: v })}
        />
        {settings.notifications && settings.streak_reminder ? (
          <div className="grid gap-3 p-3 lg:grid-cols-2">
            <Field
              label={t('app.streakHours', { count: settings.streak_reminder_hours })}
              hint={t('app.streakHoursHint')}
            >
              {(id) => (
                <input
                  id={id}
                  type="range"
                  min={1}
                  max={12}
                  step={1}
                  className="range range-primary"
                  value={settings.streak_reminder_hours}
                  onChange={(e) =>
                    void updateSettings({ streak_reminder_hours: Number(e.target.value) })
                  }
                />
              )}
            </Field>
            <Field label={t('app.streakAt')} hint={t('app.streakAtHint')}>
              {(id) => (
                <input
                  id={id}
                  type="time"
                  className="input w-full"
                  value={settings.streak_reminder_at}
                  onChange={(e) => void updateSettings({ streak_reminder_at: e.target.value })}
                />
              )}
            </Field>
          </div>
        ) : null}
        <div className="p-3">
          <Field label={t('app.theme')}>
            {(id) => (
              <select
                id={id}
                className="select w-full"
                value={settings.theme}
                onChange={(e) => void updateSettings({ theme: e.target.value })}
              >
                {THEMES.map((theme) => (
                  <option key={theme} value={theme}>
                    {theme === 'system' ? t('app.themeSystem') : theme.charAt(0).toUpperCase() + theme.slice(1)}
                  </option>
                ))}
              </select>
            )}
          </Field>
        </div>
      </div>
      <SectionTitle>{t('social.title')}</SectionTitle>
      <div className="surface p-2"><Toggle label={t('social.share')} description={t('social.shareHint')} checked={settings.share_activity} onChange={(v) => void updateSettings({ share_activity: v })} /></div>
    </>
  );
}
