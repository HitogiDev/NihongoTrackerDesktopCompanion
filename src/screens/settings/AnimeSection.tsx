import { useTranslation } from 'react-i18next';
import { HelpCircle } from 'lucide-react';
import { MpvSetupSettings } from '../../components/MpvSetup';
import { Field, Toggle } from '../../components/ui/Field';
import { useAppStore } from '../../store/useAppStore';
import type { Settings } from '../../lib/types';

export function AnimeSection({ settings }: { settings: Settings }) {
  const { t } = useTranslation(['settings', 'common']);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const trackedPlayer = useAppStore((s) => s.tracker?.anime?.player);

  return (
    <>
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
    </>
  );
}
