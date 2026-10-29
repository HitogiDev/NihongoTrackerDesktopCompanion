import { NativeHookTextFilters } from '../components/NativeHookTextFilters';
import { useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import {
  BookOpen,
  CheckCircle2,
  ExternalLink,
  LogOut,
  Trash2,
  XCircle,
  HelpCircle,
  RefreshCw,
} from 'lucide-react';
import { open } from '@tauri-apps/plugin-dialog';
import { openUrl } from '@tauri-apps/plugin-opener';
import { Button } from '../components/ui/Button';
import { DiscordSettings } from '../components/DiscordSettings';
import { PlatformSupport } from '../components/PlatformSupport';
import { UpdateSettings } from '../components/UpdateSettings';
import { BackupSettings } from '../components/BackupSettings';
import { MpvSetupSettings } from '../components/MpvSetup';
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
  const user = useAppStore((s) => s.user);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const trackedPlayer = useAppStore((s) => s.tracker?.anime?.player);
  const refreshConnection = useAppStore((s) => s.refreshConnection);
  const queryClient = useQueryClient();
  const dictionaries = useQuery({ queryKey: ['dictionaries'], queryFn: ipc.dictionaryList });
  const ankiModels = useQuery({ queryKey: ['anki-models'], queryFn: ipc.ankiModels, retry: false });
  const ankiFields = useQuery({ queryKey: ['anki-model-fields', settings?.anki_model], queryFn: () => ipc.ankiModelFields(settings?.anki_model ?? 'Basic'), retry: false });

  const [newKey, setNewKey] = useState('');
  const [keyBusy, setKeyBusy] = useState(false);
  const [reconnectBusy, setReconnectBusy] = useState(false);
  const [keyError, setKeyError] = useState<string | null>(null);
  const [dictionaryBusy, setDictionaryBusy] = useState(false);
  const [dictionaryError, setDictionaryError] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<'account' | 'tracking' | 'discord' | 'anki' | 'app'>('account');

  if (!settings) return null;

  async function replaceKey() {
    if (!settings) return;
    setKeyBusy(true);
    setKeyError(null);
    try {
      const result = await ipc.verifyApiKey(settings.api_base_url, newKey.trim());
      await updateSettings({ api_key: newKey.trim(), username: result.username ?? null });
      setNewKey('');
      await refreshConnection();
    } catch (e) {
      setKeyError(String(e));
    } finally {
      setKeyBusy(false);
    }
  }

  async function reconnect() {
    setReconnectBusy(true);
    try {
      await refreshConnection();
    } finally {
      setReconnectBusy(false);
    }
  }

  async function importDictionary() {
    setDictionaryError(null);
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [{ name: 'Yomitan dictionary', extensions: ['zip'] }],
    });
    if (typeof selected !== 'string') return;
    setDictionaryBusy(true);
    try {
      await ipc.dictionaryImport(selected);
      await queryClient.invalidateQueries({ queryKey: ['dictionaries'] });
    } catch (error) {
      setDictionaryError(String(error));
    } finally {
      setDictionaryBusy(false);
    }
  }

  const currentLanguage = settings.language || i18n.language;

  return (
    <Page title={t('title')} description={t('subtitle')}>
      <div role="tablist" aria-label={t('tabs.label')} className="tabs tabs-box tabs-sm grid grid-cols-5">
        {(['account', 'tracking', 'discord', 'anki', 'app'] as const).map((tab) => <button key={tab} type="button" role="tab" aria-selected={activeTab === tab} className={`tab ${activeTab === tab ? 'tab-active' : ''}`} onClick={() => setActiveTab(tab)}>{t(`tabs.${tab}`)}</button>)}
      </div>
      {activeTab === 'account' ? <>
      <SectionTitle>{t('account.title')}</SectionTitle>
      <div className="surface p-4 space-y-3">
        <div className="flex items-center gap-3">
          {online ? (
            <CheckCircle2 className="w-5 h-5 text-success" />
          ) : (
            <XCircle className="w-5 h-5 text-error" />
          )}
          <div className="min-w-0 flex-1">
            <p className="font-medium">{user?.username ?? settings.username ?? t('account.noUser')}</p>
            <p className="text-xs text-base-content/60">
              {online === null ? t('account.checking') : online ? t('account.connected') : t('account.disconnected')}
            </p>
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

        {settings.api_key ? (
          <Field label={t('account.currentKey')}>
            {(id) => (
              <input
                id={id}
                type="password"
                className="input w-full"
                value={'x'.repeat(24)}
                readOnly
                autoComplete="off"
              />
            )}
          </Field>
        ) : null}

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

        <div className="flex flex-wrap items-center gap-3">
          <Button appearance="outline" size="sm" loading={reconnectBusy} disabled={!settings.api_key} onClick={() => void reconnect()}>
            <RefreshCw className="h-4 w-4" />
            {t('account.reconnect')}
          </Button>
        </div>
        <Toggle
          label={t('account.autoReconnect')}
          checked={settings.auto_reconnect}
          onChange={(value) => void updateSettings({ auto_reconnect: value })}
        />

        <Button
          appearance="ghost"
          size="sm"
          onClick={() => void updateSettings({ api_key: null, username: null })}
        >
          <LogOut className="w-4 h-4" />
          {t('account.signOut')}
        </Button>
      </div>
      </> : null}

      {activeTab === 'tracking' ? <>
      <SectionTitle>{t('platform.title')}</SectionTitle>
      <PlatformSupport />
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
          <div className="grid gap-3 p-3 lg:grid-cols-2">
            <Field label={<span className="inline-flex items-center gap-1">{t('anime.mpvIpc')}<span className="tooltip tooltip-right" data-tip={t('anime.mpvIpcHelp')}><HelpCircle className="h-3.5 w-3.5 text-base-content/55" /></span></span>} hint={t('anime.mpvIpcHint')}>
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
        {settings.use_player_position ? <MpvSetupSettings current={trackedPlayer} /> : null}
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
        <div className="grid gap-3 p-3 lg:grid-cols-2">
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
          description={t('game.detectHint')}
          checked={settings.detect_games}
          onChange={(v) => void updateSettings({ detect_games: v })}
        />
        <p className="p-3 text-sm text-base-content/60">{t('game.rules')}</p>
      </div>

      <NativeHookTextFilters />
      <SectionTitle>{t('readingOverlay:title')}</SectionTitle>
      <div className="surface p-4">
        <Button appearance="outline" onClick={() => void ipc.openAuxWindow('reading-overlay-settings')}>{t('readingOverlay:openSettings')}</Button>
      </div>
      </> : null}

      {activeTab === 'discord' ? <>
      <DiscordSettings settings={settings} onChange={(patch) => void updateSettings(patch)} />
      </> : null}

      {activeTab === 'anki' ? <>
      <SectionTitle>{t('anki.title')}</SectionTitle>
      <div className="surface p-4 space-y-3">
        <Field label={t('anki.deck')}>
          {(id) => <input id={id} className="input w-full" value={settings.anki_deck} onChange={(e) => void updateSettings({ anki_deck: e.target.value })} />}
        </Field>
        <Field label={t('anki.noteType')} hint={t('anki.noteTypeHint')}>
          {(id) => <select id={id} className="select w-full" value={settings.anki_model} onChange={(e) => void updateSettings({ anki_model: e.target.value, anki_field_sources: {} })}>
            {(ankiModels.data ?? []).map((model) => <option key={model} value={model}>{model}</option>)}
            {!(ankiModels.data ?? []).includes(settings.anki_model) ? <option value={settings.anki_model}>{settings.anki_model}</option> : null}
          </select>}
        </Field>
        {ankiModels.isError ? <p className="text-xs text-base-content/60">{t('anki.unavailable')}</p> : null}
        <div className="grid gap-2 lg:grid-cols-2">
          {(ankiFields.data ?? []).map((field) => <Field key={field} label={t('anki.field', { field })}>
            {(id) => <select id={id} className="select w-full" value={settings.anki_field_sources[field] ?? ''} onChange={(e) => void updateSettings({ anki_field_sources: { ...settings.anki_field_sources, [field]: e.target.value } })}>
              {['word', 'reading', 'meaning', 'sentence', 'media', ''].map((source) => <option key={source || 'empty'} value={source}>{t(`anki.sources.${source || 'empty'}`)}</option>)}
            </select>}
          </Field>)}
        </div>
      </div>

      <SectionTitle>{t('dictionary.title')}</SectionTitle>
      <div className="surface p-4 space-y-3">
        <p className="text-sm text-base-content/70">{t('dictionary.hint')}</p>
        <Button size="sm" variant="primary" loading={dictionaryBusy} onClick={() => void importDictionary()}>
          <BookOpen className="w-4 h-4" />
          {t('dictionary.import')}
        </Button>
        {dictionaryError ? <p className="text-sm text-error selectable">{dictionaryError}</p> : null}
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
                    void ipc.dictionaryRemove(dictionary.id).then(() =>
                      queryClient.invalidateQueries({ queryKey: ['dictionaries'] })
                    )
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
      </> : null}

      {activeTab === 'app' ? <>
      <SectionTitle>{t('app.title')}</SectionTitle>
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
      <SectionTitle>{t('common:update.title')}</SectionTitle>
      <UpdateSettings />
      <SectionTitle>{t('backup.section')}</SectionTitle>
      <BackupSettings />
      <SectionTitle>{t('app.advanced')}</SectionTitle>
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
      </> : null}
    </Page>
  );
}
