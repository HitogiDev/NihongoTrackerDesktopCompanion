import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { getVersion } from '@tauri-apps/api/app';
import { openUrl } from '@tauri-apps/plugin-opener';
import { Download, RefreshCw } from 'lucide-react';
import { Button } from './ui/Button';
import { Toggle } from './ui/Field';
import { RELEASES_URL, useUpdater } from '../hooks/useUpdater';
import { useAppStore } from '../store/useAppStore';

/** Settings → App: current version, automatic checks, and the update itself. */
export function UpdateSettings() {
  const { t } = useTranslation('common');
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const { update, checking, checked, installing, progress, error, check, install } = useUpdater();
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    void getVersion().then(setVersion).catch(() => undefined);
  }, []);

  if (!settings) return null;

  return (
    <div className="surface divide-y divide-base-300/50 p-2">
      <div className="flex flex-wrap items-center gap-3 p-3">
        <div className="min-w-0 flex-1">
          <p className="font-medium">{version ? t('update.current', { version }) : t('update.title')}</p>
          <p className="text-xs text-base-content/60">
            {update
              ? t('update.available', { version: update.version })
              : checked ? t('update.upToDate') : t('update.notChecked')}
          </p>
        </div>
        {update ? (
          <Button size="sm" variant="primary" loading={installing} disabled={installing} onClick={() => void install()}>
            <Download className="h-4 w-4" />
            {installing
              ? t('update.installing', { percent: progress ?? 0 })
              : update.can_install ? t('update.install') : t('update.download')}
          </Button>
        ) : (
          <Button size="sm" appearance="outline" loading={checking} disabled={checking} onClick={() => void check()}>
            <RefreshCw className="h-4 w-4" />
            {checking ? t('update.checking') : t('update.check')}
          </Button>
        )}
      </div>
      {update && !update.can_install ? (
        <p className="p-3 text-xs text-base-content/60">{t('update.manualHint')}</p>
      ) : null}
      {update?.notes ? (
        <details className="p-3">
          <summary className="text-sm font-medium">{t('update.notes')}</summary>
          <p className="selectable mt-2 whitespace-pre-wrap text-sm text-base-content/75">{update.notes}</p>
          <button type="button" className="link link-hover mt-2 text-xs" onClick={() => void openUrl(RELEASES_URL).catch(() => undefined)}>
            {t('update.openRelease')}
          </button>
        </details>
      ) : null}
      {error ? (
        <p role="alert" className="p-3 text-sm text-error break-words">
          {t(error.kind === 'check' ? 'update.checkFailed' : 'update.installFailed', { message: error.message })}
        </p>
      ) : null}
      <Toggle
        label={t('update.auto')}
        description={t('update.autoHint')}
        checked={settings.auto_update_check}
        onChange={(value) => void updateSettings({ auto_update_check: value })}
      />
    </div>
  );
}

/** Sidebar footer notice, only while a newer release is known. */
export function UpdateNotice() {
  const { t } = useTranslation('common');
  const { update, installing, progress, install } = useUpdater();
  if (!update) return null;
  return (
    <button
      type="button"
      className="flex w-full items-center gap-2 rounded-field bg-primary/10 px-2.5 py-2 text-left text-xs text-primary hover:bg-primary/15"
      disabled={installing}
      title={update.can_install ? t('update.install') : t('update.download')}
      onClick={() => void install()}
    >
      <Download className="h-3.5 w-3.5 shrink-0" />
      <span className="min-w-0 flex-1 truncate">
        {installing ? t('update.installing', { percent: progress ?? 0 }) : t('update.available', { version: update.version })}
      </span>
    </button>
  );
}
