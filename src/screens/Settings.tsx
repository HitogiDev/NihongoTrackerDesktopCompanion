import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import {
  CheckCircle2,
  ExternalLink,
  FolderOpen,
  LogOut,
  Puzzle,
  XCircle,
} from 'lucide-react';
import { openUrl } from '@tauri-apps/plugin-opener';
import { Button } from '../components/ui/Button';
import { DiscordSettings } from '../components/DiscordSettings';
import { Field, Toggle } from '../components/ui/Field';
import { Page, SectionTitle } from '../components/ui/Page';
import { ipc } from '../lib/ipc';
import {
  LANGUAGE_LABELS,
  SUPPORTED_LANGUAGES,
  type SupportedLanguage,
} from '../i18n/languages';
import { useAppStore } from '../store/useAppStore';

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

export default function SettingsScreen() {
  const { t, i18n } = useTranslation(['settings', 'common']);
  const settings = useAppStore((s) => s.settings);
  const online = useAppStore((s) => s.online);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const refreshConnection = useAppStore((s) => s.refreshConnection);

  const extensions = useQuery({ queryKey: ['extensions'], queryFn: ipc.listExtensions });

  const [newKey, setNewKey] = useState('');
  const [keyBusy, setKeyBusy] = useState(false);
  const [keyError, setKeyError] = useState<string | null>(null);

  if (!settings) return null;

  async function replaceKey() {
    if (!settings) return;
    setKeyBusy(true);
    setKeyError(null);
    try {
      const result = await ipc.verifyApiKey(settings.api_base_url, newKey.trim());
      const user = result.user as { username?: string } | undefined;
      await updateSettings({ api_key: newKey.trim(), username: user?.username ?? null });
      setNewKey('');
      await refreshConnection();
    } catch (e) {
      setKeyError(String(e));
    } finally {
      setKeyBusy(false);
    }
  }

  const currentLanguage = settings.language || i18n.language;

  return (
    <Page title={t('title')} description={t('subtitle')}>
      <SectionTitle>{t('account.title')}</SectionTitle>
      <div className="surface p-4 space-y-3">
        <div className="flex items-center gap-3">
          {online ? (
            <CheckCircle2 className="w-5 h-5 text-success" />
          ) : (
            <XCircle className="w-5 h-5 text-error" />
          )}
          <div className="min-w-0 flex-1">
            <p className="font-medium">{settings.username ?? t('account.noUser')}</p>
            <p className="text-xs text-base-content/60 truncate">{settings.api_base_url}</p>
          </div>
          <Button
            size="sm"
            appearance="outline"
            onClick={() => void openUrl(`${settings.api_base_url}/settings`)}
          >
            <ExternalLink className="w-4 h-4" />
            {t('account.web')}
          </Button>
        </div>

        <Field label={t('account.server')}>
          {(id) => (
            <input
              id={id}
              className="input w-full"
              value={settings.api_base_url}
              onChange={(e) => void updateSettings({ api_base_url: e.target.value })}
            />
          )}
        </Field>

        <Field label={t('account.newKey')} error={keyError}>
          {(id) => (
            <div className="flex gap-2">
              <input
                id={id}
                type="password"
                className="input flex-1"
                value={newKey}
                onChange={(e) => setNewKey(e.target.value)}
                placeholder="ntk_…"
              />
              <Button
                variant="primary"
                loading={keyBusy}
                disabled={!newKey.trim()}
                onClick={() => void replaceKey()}
              >
                {t('account.replace')}
              </Button>
            </div>
          )}
        </Field>

        <Button
          appearance="ghost"
          size="sm"
          onClick={() => void updateSettings({ api_key: null, username: null })}
        >
          <LogOut className="w-4 h-4" />
          {t('account.signOut')}
        </Button>
      </div>

      <SectionTitle>{t('anime.title')}</SectionTitle>
      <div className="surface p-2 divide-y divide-base-300/50">
        <Toggle
          label={t('anime.detect')}
          description={t('anime.detectHint')}
          checked={settings.detect_players}
          onChange={(v) => void updateSettings({ detect_players: v })}
        />
        <Toggle
          label={t('anime.usePosition')}
          description={t('anime.usePositionHint')}
          checked={settings.use_player_position}
          onChange={(v) => void updateSettings({ use_player_position: v })}
        />
        {settings.use_player_position ? (
          <div className="p-3 grid gap-3 sm:grid-cols-2">
            <Field label={t('anime.mpvIpc')} hint={t('anime.mpvIpcHint')}>
              {(id) => (
                <input
                  id={id}
                  className="input w-full font-mono text-sm"
                  value={settings.mpv_ipc_path}
                  onChange={(e) => void updateSettings({ mpv_ipc_path: e.target.value })}
                />
              )}
            </Field>
            <Field label={t('anime.mpcPort')} hint={t('anime.mpcPortHint')}>
              {(id) => (
                <input
                  id={id}
                  type="number"
                  min={1}
                  max={65535}
                  className="input w-full"
                  value={settings.mpc_web_port}
                  onChange={(e) => void updateSettings({ mpc_web_port: Number(e.target.value) })}
                />
              )}
            </Field>
          </div>
        ) : null}
        <Toggle
          label={t('anime.autoLog')}
          description={t('anime.autoLogHint')}
          checked={settings.anime_auto_log}
          onChange={(v) => void updateSettings({ anime_auto_log: v })}
        />
        <Toggle
          label={t('anime.logTime')}
          description={t('anime.logTimeHint')}
          checked={settings.anime_log_time}
          onChange={(v) => void updateSettings({ anime_log_time: v })}
        />
        <div className="p-3 grid gap-3 sm:grid-cols-2">
          <Field label={t('anime.threshold', { percent: settings.anime_threshold_percent })}>
            {(id) => (
              <input
                id={id}
                type="range"
                min={10}
                max={100}
                step={5}
                className="range range-primary"
                value={settings.anime_threshold_percent}
                onChange={(e) =>
                  void updateSettings({ anime_threshold_percent: Number(e.target.value) })
                }
              />
            )}
          </Field>
          <Field label={t('anime.episodeMinutes')}>
            {(id) => (
              <input
                id={id}
                type="number"
                min={1}
                max={180}
                className="input w-full"
                value={settings.anime_episode_minutes}
                onChange={(e) =>
                  void updateSettings({ anime_episode_minutes: Number(e.target.value) })
                }
              />
            )}
          </Field>
        </div>
      </div>

      <SectionTitle>{t('vn.title')}</SectionTitle>
      <div className="surface p-2 divide-y divide-base-300/50">
        <Toggle
          label={t('vn.detect')}
          description={t('vn.detectHint')}
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
          description={t('vn.confirmHint')}
          checked={settings.vn_confirm_before_log}
          onChange={(v) => void updateSettings({ vn_confirm_before_log: v })}
        />
        <div className="p-3">
          <Field
            label={t('vn.afk', { seconds: settings.afk_timeout_secs })}
            hint={t('vn.afkHint')}
          >
            {(id) => (
              <input
                id={id}
                type="range"
                min={0}
                max={600}
                step={10}
                className="range range-primary"
                value={settings.afk_timeout_secs}
                onChange={(e) => void updateSettings({ afk_timeout_secs: Number(e.target.value) })}
              />
            )}
          </Field>
        </div>
      </div>

      <DiscordSettings settings={settings} onChange={(patch) => void updateSettings(patch)} />

      <SectionTitle>{t('extensions.title')}</SectionTitle>
      <div className="surface p-2 divide-y divide-base-300/50">
        <Toggle
          label={t('extensions.enabled')}
          description={t('extensions.enabledHint')}
          checked={settings.extensions_enabled}
          onChange={(v) => void updateSettings({ extensions_enabled: v })}
        />
        <div className="p-3 space-y-3">
          <p className="text-sm text-base-content/70">{t('extensions.body')}</p>
          <div className="flex flex-wrap items-center gap-2">
            <Button size="sm" appearance="outline" onClick={() => void ipc.openExtensionsFolder()}>
              <FolderOpen className="w-4 h-4" />
              {t('extensions.openFolder')}
            </Button>
            <Button
              size="sm"
              appearance="ghost"
              onClick={() => void openUrl('https://yomitan.wiki/')}
            >
              <ExternalLink className="w-4 h-4" />
              Yomitan
            </Button>
            <Button
              size="sm"
              appearance="ghost"
              onClick={() => void openUrl('https://github.com/Sirush/JitenReader')}
            >
              <ExternalLink className="w-4 h-4" />
              JitenReader
            </Button>
          </div>

          {(extensions.data ?? []).length === 0 ? (
            <p className="text-xs text-base-content/50">{t('extensions.none')}</p>
          ) : (
            <ul className="space-y-1.5">
              {(extensions.data ?? []).map((ext) => (
                <li
                  key={ext.id}
                  className="flex items-center gap-3 px-3 py-2 rounded-field bg-base-200/60"
                >
                  <Puzzle className="w-4 h-4 text-base-content/50 shrink-0" />
                  <span className="min-w-0 flex-1">
                    <span className="block text-sm font-medium truncate">{ext.name}</span>
                    <span className="block text-xs text-base-content/50 font-mono truncate">
                      v{ext.version} · MV{ext.manifest_version} · {ext.id}
                    </span>
                  </span>
                  {ext.options_url ? (
                    <Button
                      size="xs"
                      appearance="outline"
                      onClick={() => void ipc.openExtensionOptions(ext.options_url!)}
                    >
                      {t('extensions.settings')}
                    </Button>
                  ) : null}
                </li>
              ))}
            </ul>
          )}
          <p className="text-xs text-base-content/50">{t('extensions.restart')}</p>
        </div>
      </div>

      <SectionTitle>{t('app.title')}</SectionTitle>
      <div className="surface p-2 divide-y divide-base-300/50">
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
          <div className="p-3 grid gap-3 sm:grid-cols-2">
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
        <div className="p-3 grid gap-3 sm:grid-cols-2">
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
                    {theme}
                  </option>
                ))}
              </select>
            )}
          </Field>
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
    </Page>
  );
}
