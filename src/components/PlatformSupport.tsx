import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { CheckCircle2, MinusCircle, ShieldAlert } from 'lucide-react';
import { Button } from './ui/Button';
import { ipc } from '../lib/ipc';
import type { Capabilities } from '../lib/types';

const ROWS = [
  'window_titles',
  'media_sessions',
  'foreground',
  'idle',
  'screenshots',
  'actionable_notifications',
  'native_hook',
  'ctrl_shield',
] as const satisfies ReadonlyArray<keyof Capabilities>;

/**
 * What detection can see on this OS, row by row, so a missing feature says why
 * instead of the whole tracker reading as "unsupported". On macOS it offers the
 * Screen Recording prompt that unlocks window titles.
 */
export function PlatformSupport() {
  const { t } = useTranslation('settings');
  const capabilities = useQuery({
    queryKey: ['platform-capabilities'],
    queryFn: ipc.platformCapabilities,
    // A permission granted in System Settings shows up when the user comes back.
    refetchOnWindowFocus: true,
  });
  const caps = capabilities.data;
  if (!caps) return null;

  const platform = caps.session ? `${t(`platform.os.${caps.os}`, caps.os)} · ${caps.session}` : t(`platform.os.${caps.os}`, caps.os);
  // MPRIS only exists on Linux; elsewhere the row would only be noise.
  const rows = ROWS.filter((key) => key !== 'media_sessions' || caps.os === 'linux');
  const missing = rows.filter((key) => !caps[key]);
  const available = rows.filter((key) => caps[key]);

  // What is missing explains why something does not work, so it stays in view;
  // the list of what works folds away.
  return (
    <div className="surface p-4 space-y-3">
      <div className="flex items-center gap-2">
        {missing.length === 0 ? <CheckCircle2 className="h-4 w-4 shrink-0 text-success" aria-hidden="true" /> : null}
        <p className="text-sm font-medium">
          {platform}
          {missing.length === 0 ? <span className="font-normal text-base-content/60"> · {t('platform.allAvailable')}</span> : null}
        </p>
      </div>
      {missing.length > 0 ? (
        <ul className="space-y-2">
          {missing.map((key) => (
            <li key={key} className="flex items-start gap-2.5 text-sm">
              <MinusCircle className="mt-0.5 h-4 w-4 shrink-0 text-base-content/40" aria-hidden="true" />
              <div className="min-w-0">
                <p className="text-base-content/60">
                  {t(`platform.features.${key}`)}
                  <span className="sr-only">: {t('platform.unavailable')}</span>
                </p>
                <p className="text-xs text-base-content/60">{t(`platform.missing.${key}`, { context: caps.os })}</p>
              </div>
            </li>
          ))}
        </ul>
      ) : null}
      <details>
        <summary className="cursor-pointer text-xs text-base-content/60">{t('platform.availableList', { count: available.length })}</summary>
        <ul className="mt-2 space-y-2">
          {available.map((key) => (
            <li key={key} className="flex items-start gap-2.5 text-sm">
              <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0 text-success" aria-hidden="true" />
              <p>
                {t(`platform.features.${key}`)}
                <span className="sr-only">: {t('platform.available')}</span>
              </p>
            </li>
          ))}
        </ul>
      </details>
      {caps.needs_screen_recording ? (
        <div className="surface-muted flex items-start gap-3 p-3">
          <ShieldAlert className="mt-0.5 h-4 w-4 shrink-0 text-warning" aria-hidden="true" />
          <p className="min-w-0 flex-1 text-xs text-base-content/70">{t('platform.screenRecordingHint')}</p>
          <Button size="sm" appearance="outline" onClick={() => void ipc.requestWindowTitlesPermission().then(() => capabilities.refetch())}>
            {t('platform.screenRecordingAction')}
          </Button>
        </div>
      ) : null}
    </div>
  );
}
