import { useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { Download, Pencil, Server, Trash2 } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Field, Toggle } from '../../components/ui/Field';
import { ipc } from '../../lib/ipc';
import { displayUrl, formatBytes } from '../../lib/manga';
import type { ExtMode, Settings } from '../../lib/types';
import { useAppStore } from '../../store/useAppStore';
import { toast } from '../../store/useToastStore';
import { ExtInstallModal } from '../manga/ext/ExtInstallModal';
import { ExtCustomModal, useCustomServer } from '../manga/ext/ExtServer';
import { EXT_STATUS_QUERY, extErrorText, isInstalling, useExtInstallStore, useExtStatus } from '../manga/ext/useExtensions';

const MODES: ExtMode[] = ['managed', 'custom'];

/** Ajustes → Lector de manga → Extensiones: where they run, adult sources, idle stop. */
export function ExtensionsSettings({ settings }: { settings: Settings }) {
  const { t, i18n } = useTranslation(['settings', 'manga', 'common']);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const queryClient = useQueryClient();
  const status = useExtStatus().data;
  const custom = useCustomServer().data;
  const showInstall = useExtInstallStore((s) => s.show);
  const [customOpen, setCustomOpen] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [removing, setRemoving] = useState(false);
  const mode = settings.manga_ext_mode;
  const idle = Number(settings.manga_ext_idle_minutes);
  const installing = isInstalling(status);

  const uninstall = async () => {
    setRemoving(true);
    try {
      await ipc.mangaExtUninstall();
      await queryClient.invalidateQueries({ queryKey: EXT_STATUS_QUERY });
      toast.success(t('manga.extRemoved'));
    } catch (error) {
      toast.error(extErrorText(error));
    } finally {
      setRemoving(false);
      setConfirming(false);
    }
  };

  return (
    <>
      <div className="p-3">
        <Field label={t('manga.ext')} hint={t(mode === 'managed' ? 'manga.extManagedHint' : 'manga.extCustomHint')}>
          <div className="join flex flex-wrap" role="radiogroup" aria-label={t('manga.ext')}>
            {MODES.map((option) => (
              <button
                key={option}
                type="button"
                role="radio"
                aria-checked={mode === option}
                className={mode === option ? 'join-item btn btn-primary btn-sm' : 'join-item btn btn-outline btn-sm'}
                onClick={async () => {
                  if (option === mode) return;
                  await updateSettings({ manga_ext_mode: option });
                  await queryClient.invalidateQueries({ queryKey: ['manga', 'ext'] });
                }}
              >
                {t(`manga:ext.mode.${option}`)}
                {option === 'managed' ? <span className="badge badge-xs badge-ghost">{t('manga:ext.mode.recommended')}</span> : null}
              </button>
            ))}
          </div>
        </Field>
        {status ? (
          <div className="mt-2 flex flex-wrap items-center gap-3">
            <div className="min-w-48 flex-1 text-sm">
              {mode === 'managed' ? (
                status.installed ? (
                  <p>
                    <span className="badge badge-success badge-soft badge-sm mr-2">{t('manga:ext.state.installed')}</span>
                    <span className="text-base-content/65">{t('manga.extInstalled', { size: formatBytes(Number(status.installed_bytes), i18n.language), state: t(`manga:ext.server.phase.${status.phase}`) })}</span>
                  </p>
                ) : installing ? (
                  <p className="text-base-content/65">{t(`manga:ext.install.phase.${status.install!.phase}`)}</p>
                ) : (
                  <p className="text-base-content/65">{status.supported ? t('manga.extMissing', { size: formatBytes(Number(status.download_bytes), i18n.language) }) : t('manga:ext.errors.unsupported')}</p>
                )
              ) : custom ? (
                <p className="flex min-w-0 items-center gap-1.5 text-base-content/70">
                  <Server className="h-4 w-4 shrink-0" aria-hidden="true" />
                  <span className="selectable truncate font-mono text-xs">{displayUrl(custom.url)}</span>
                  {custom.auth === 'basic' ? <span className="badge badge-ghost badge-xs">{t('manga:webdav.form.authKind.basic')}</span> : null}
                </p>
              ) : (
                <p className="text-base-content/65">{t('manga.extNoCustom')}</p>
              )}
              {status.phase === 'error' ? <p className="mt-1 text-xs text-error">{extErrorText(status.error)}</p> : null}
            </div>
            {mode === 'managed' ? (
              <>
                {!status.installed ? (
                  <Button variant="primary" size="sm" disabled={!status.supported} onClick={showInstall}>
                    <Download className="h-4 w-4" />
                    {installing ? t('manga.ocrShow') : t('manga.extInstall')}
                  </Button>
                ) : null}
                {Number(status.installed_bytes) > 0 && !installing ? (
                  confirming ? (
                    <span className="flex items-center gap-1">
                      <Button variant="error" size="sm" loading={removing} onClick={() => void uninstall()}>{removing ? null : <Trash2 className="h-4 w-4" />}{t('manga.extRemoveConfirm')}</Button>
                      <Button appearance="ghost" size="sm" onClick={() => setConfirming(false)}>{t('common:cancel')}</Button>
                    </span>
                  ) : (
                    <Button appearance="outline" size="sm" onClick={() => setConfirming(true)}><Trash2 className="h-4 w-4" />{t('manga.extRemove')}</Button>
                  )
                ) : null}
              </>
            ) : (
              <Button appearance="outline" size="sm" onClick={() => setCustomOpen(true)}>
                <Pencil className="h-4 w-4" />
                {custom ? t('manga.extEditCustom') : t('manga.extSetCustom')}
              </Button>
            )}
          </div>
        ) : null}
        {confirming ? <p className="mt-2 text-xs text-base-content/60">{t('manga.extRemoveHint')}</p> : null}
      </div>
      <Toggle
        label={t('manga.extAdult')}
        description={t('manga.extAdultHint')}
        checked={settings.manga_ext_show_adult}
        onChange={async (v) => {
          await updateSettings({ manga_ext_show_adult: v });
          await queryClient.invalidateQueries({ queryKey: ['manga', 'ext'] });
        }}
      />
      {mode === 'managed' ? (
        <div className="p-3">
          <Field label={t('manga.extIdle')} hint={t('manga.extIdleHint')}>
            {(id) => (
              <input
                id={id}
                type="number"
                min={0}
                max={240}
                step={1}
                className="input w-32"
                value={idle}
                onChange={(e) => void updateSettings({ manga_ext_idle_minutes: Math.max(0, Number(e.target.value)) })}
              />
            )}
          </Field>
        </div>
      ) : null}
      <ExtInstallModal />
      <ExtCustomModal open={customOpen} onClose={() => setCustomOpen(false)} />
    </>
  );
}
