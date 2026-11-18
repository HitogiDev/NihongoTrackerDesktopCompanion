import { useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { AlertTriangle, Check, Coffee, Download, ExternalLink, HardDrive, Puzzle, Server, ShieldAlert, X } from 'lucide-react';
import { openUrl } from '@tauri-apps/plugin-opener';
import { Button } from '../../../components/ui/Button';
import { Modal } from '../../../components/ui/Modal';
import { ipc } from '../../../lib/ipc';
import { formatBytes } from '../../../lib/manga';
import type { ExtComponent, ExtStatus } from '../../../lib/types';
import { EXT_STATUS_QUERY, isInstalling, useExtInstallStore, useExtStatus } from './useExtensions';

const ICON = { server: Server, java: Coffee } as const;
const SOURCE_URL = 'https://github.com/Suwayomi/Suwayomi-Server';
/** Room to spare on the disk beyond the files themselves. */
const SPACE_MARGIN = 1.1;

function ComponentRow({ component }: { component: ExtComponent }) {
  const { t, i18n } = useTranslation('manga');
  const Icon = ICON[component.kind as keyof typeof ICON] ?? Server;
  return (
    <li className="flex items-center gap-3 px-3 py-2.5">
      <span className="grid h-9 w-9 shrink-0 place-items-center rounded-field bg-base-100 text-base-content/70 shadow-sm">
        <Icon className="h-4 w-4" aria-hidden="true" />
      </span>
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-medium">{t(`ext.install.component.${component.kind}`, { version: component.version })}</p>
        <p className="truncate text-xs text-base-content/55">
          {t(`ext.install.componentHint.${component.kind}`, { size: formatBytes(Number(component.size), i18n.language) })}
        </p>
      </div>
      <span className="badge badge-ghost badge-sm hidden shrink-0 font-mono sm:inline-flex">{component.license}</span>
      <span className="w-20 shrink-0 text-right text-xs tabular-nums text-base-content/70">
        {component.present ? (
          <span className="inline-flex items-center gap-1 text-success"><Check className="h-3.5 w-3.5" aria-hidden="true" />{t('ext.install.present')}</span>
        ) : formatBytes(Number(component.bytes), i18n.language)}
      </span>
    </li>
  );
}

function Progress({ status }: { status: ExtStatus }) {
  const { t, i18n } = useTranslation('manga');
  const install = status.install!;
  const total = Number(install.total);
  const received = Number(install.received);
  const percent = total > 0 ? Math.min(100, Math.round((received / total) * 100)) : 0;
  const running = isInstalling(status);
  return (
    <div className="space-y-4" aria-live="polite">
      {install.phase === 'failed' ? (
        <div role="alert" className="alert alert-error alert-soft">
          <AlertTriangle className="h-4 w-4" aria-hidden="true" />
          <span>{t(`ext.install.errors.${install.error ?? 'server'}`, { defaultValue: t('ext.install.errors.server') })}</span>
        </div>
      ) : install.phase === 'done' ? (
        <div className="flex items-center gap-3 rounded-box bg-success/10 p-4 text-success">
          <span className="grid h-10 w-10 place-items-center rounded-full bg-success/15"><Check className="h-5 w-5" aria-hidden="true" /></span>
          <div className="text-base-content">
            <p className="font-semibold">{t('ext.install.doneTitle')}</p>
            <p className="text-sm text-base-content/65">{t('ext.install.doneBody')}</p>
          </div>
        </div>
      ) : null}
      {install.phase !== 'done' ? (
        <div className="surface-muted space-y-2 p-4">
          <div className="flex items-baseline justify-between gap-3 text-sm">
            <span className="min-w-0 truncate font-medium">{t(`ext.install.phase.${install.phase}`)}</span>
            <span className="shrink-0 tabular-nums text-base-content/70">{percent}%</span>
          </div>
          <progress className="progress progress-primary h-2 w-full" value={install.phase === 'unpacking' ? undefined : percent} max={100} aria-label={t('ext.install.progress')} />
          <div className="flex justify-between gap-3 text-xs tabular-nums text-base-content/55">
            <span className="min-w-0 truncate font-mono">{install.file}</span>
            <span className="shrink-0">{t('ext.install.of', { received: formatBytes(received, i18n.language), total: formatBytes(total, i18n.language) })}</span>
          </div>
        </div>
      ) : null}
      {running ? <p className="text-xs text-base-content/55">{t('ext.install.background')}</p> : null}
      {install.phase === 'cancelled' || install.phase === 'failed' ? <p className="text-xs text-base-content/55">{t('ext.install.resumes')}</p> : null}
    </div>
  );
}

/**
 * Before anything is downloaded: what the managed server is (a Java program
 * by a third party), what it takes and where, and that extensions are other
 * people's code running in it. Nothing is downloaded or started until the
 * user says so; then the same dialog follows the download.
 */
export function ExtInstallModal() {
  const { t, i18n } = useTranslation(['manga', 'common']);
  const open = useExtInstallStore((s) => s.open);
  const close = useExtInstallStore((s) => s.close);
  const status = useExtStatus().data;
  const queryClient = useQueryClient();
  const [starting, setStarting] = useState(false);
  if (!status) return null;
  const installing = isInstalling(status);
  const showProgress = Boolean(status.install);
  const download = Number(status.download_bytes);
  const disk = Number(status.disk_bytes);
  const free = status.free_bytes === null ? null : Number(status.free_bytes);
  const tight = free !== null && free < disk * SPACE_MARGIN && !status.installed;

  const start = async () => {
    setStarting(true);
    try {
      await ipc.mangaExtInstall();
      await queryClient.invalidateQueries({ queryKey: EXT_STATUS_QUERY });
    } finally {
      setStarting(false);
    }
  };

  const actions = showProgress && status.install ? (
    installing ? (
      <>
        <Button appearance="ghost" onClick={close}>{t('ext.install.hide')}</Button>
        <Button appearance="outline" onClick={() => void ipc.mangaExtCancelInstall()}><X className="h-4 w-4" />{t('ext.install.cancel')}</Button>
      </>
    ) : status.install.phase === 'done' ? (
      <Button variant="primary" onClick={close}>{t('ext.install.close')}</Button>
    ) : (
      <>
        <Button appearance="ghost" onClick={close}>{t('common:cancel')}</Button>
        <Button variant="primary" loading={starting} onClick={() => void start()}>{starting ? null : <Download className="h-4 w-4" />}{t('ext.install.resume')}</Button>
      </>
    )
  ) : (
    <>
      <Button appearance="ghost" onClick={close}>{t('common:cancel')}</Button>
      <Button variant="primary" loading={starting} disabled={!status.supported || download === 0} onClick={() => void start()}>
        {starting ? null : <Download className="h-4 w-4" />}
        {t('ext.install.accept', { size: formatBytes(download, i18n.language) })}
      </Button>
    </>
  );

  return (
    <Modal open={open} onClose={close} title={t('ext.install.title')} size="lg" actions={actions}>
      {showProgress && status.install ? <Progress status={status} /> : !status.supported ? (
        <div role="alert" className="alert alert-warning alert-soft"><AlertTriangle className="h-4 w-4" aria-hidden="true" /><span>{t('ext.errors.unsupported')}</span></div>
      ) : (
        <div className="space-y-5">
          <p className="text-sm text-base-content/75">{t('ext.install.intro')}</p>
          <ul className="surface-muted divide-y divide-base-300/60 overflow-hidden">
            {status.components.map((component) => <ComponentRow key={component.kind} component={component} />)}
          </ul>
          <dl className="grid gap-3 text-sm sm:grid-cols-2">
            <div className="flex items-center gap-2">
              <Download className="h-4 w-4 text-base-content/50" aria-hidden="true" />
              <dt className="text-base-content/60">{t('ext.install.download')}</dt>
              <dd className="ml-auto font-semibold tabular-nums">{formatBytes(download, i18n.language)}</dd>
            </div>
            <div className="flex items-center gap-2">
              <HardDrive className="h-4 w-4 text-base-content/50" aria-hidden="true" />
              <dt className="text-base-content/60">{t('ext.install.disk')}</dt>
              <dd className="ml-auto font-semibold tabular-nums">{t('ext.install.diskValue', { size: formatBytes(disk, i18n.language) })}</dd>
            </div>
            <div className="flex items-center gap-2 sm:col-span-2">
              <HardDrive className="h-4 w-4 text-base-content/50" aria-hidden="true" />
              <dt className="text-base-content/60">{t('ext.install.free')}</dt>
              <dd className={tight ? 'ml-auto font-semibold tabular-nums text-error' : 'ml-auto font-semibold tabular-nums'}>
                {free === null ? t('ext.install.unknown') : formatBytes(free, i18n.language)}
              </dd>
            </div>
            <div className="sm:col-span-2">
              <dt className="text-xs text-base-content/55">{t('ext.install.where')}</dt>
              <dd className="selectable truncate font-mono text-xs text-base-content/75" title={status.dir}>{status.dir}</dd>
            </div>
          </dl>
          {tight ? (
            <div role="alert" className="alert alert-warning alert-soft text-sm"><AlertTriangle className="h-4 w-4" aria-hidden="true" /><span>{t('ext.install.noRoom')}</span></div>
          ) : null}
          <div className="space-y-2 text-xs text-base-content/65">
            <p className="flex items-start gap-2">
              <Server className="mt-0.5 h-4 w-4 shrink-0 text-base-content/50" aria-hidden="true" />
              <span>{t('ext.install.local')}</span>
            </p>
            <p className="flex items-start gap-2">
              <Puzzle className="mt-0.5 h-4 w-4 shrink-0 text-warning" aria-hidden="true" />
              <span>{t('ext.install.thirdParty')}</span>
            </p>
            <p className="flex items-start gap-2">
              <ShieldAlert className="mt-0.5 h-4 w-4 shrink-0 text-base-content/50" aria-hidden="true" />
              <span>{t('ext.install.license')}{' '}
                <button type="button" className="link link-hover inline-flex items-center gap-0.5" onClick={() => void openUrl(SOURCE_URL)}>
                  {t('ext.install.sourceLink')}<ExternalLink className="h-3 w-3" aria-hidden="true" />
                </button>
              </span>
            </p>
          </div>
        </div>
      )}
    </Modal>
  );
}
