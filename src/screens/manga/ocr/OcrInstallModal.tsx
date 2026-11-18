import { useEffect, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { AlertTriangle, Check, Cpu, Download, Gauge, HardDrive, ScanSearch, ScanText, ShieldCheck, X } from 'lucide-react';
import { Button } from '../../../components/ui/Button';
import { Modal } from '../../../components/ui/Modal';
import { ipc } from '../../../lib/ipc';
import { formatBytes } from '../../../lib/manga';
import type { OcrComponent, OcrStatus, OcrVariant } from '../../../lib/types';
import { useAppStore } from '../../../store/useAppStore';
import { isDownloading, OCR_QUERY, ocrErrorText, useOcrInstallStore, useOcrStatus } from './useOcr';

const COMPONENT_ICON = { runtime: Cpu, detector: ScanSearch, recognizer: ScanText, meikiDetector: ScanSearch, meikiRecognizer: ScanText } as const;
const VARIANTS: OcrVariant[] = ['int8', 'fp16'];
/** Room to spare on the disk beyond the files themselves. */
const SPACE_MARGIN = 1.1;

function ComponentRow({ component, status }: { component: OcrComponent; status: OcrStatus }) {
  const { t, i18n } = useTranslation('manga');
  const Icon = COMPONENT_ICON[component.kind as keyof typeof COMPONENT_ICON] ?? Cpu;
  return (
    <li className="flex items-center gap-3 px-3 py-2.5">
      <span className="grid h-9 w-9 shrink-0 place-items-center rounded-field bg-base-100 text-base-content/70 shadow-sm">
        <Icon className="h-4 w-4" aria-hidden="true" />
      </span>
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-medium">{t(`ocr.install.component.${component.kind}`, { version: status.runtime_version })}</p>
        <p className="truncate text-xs text-base-content/55">{t(`ocr.install.componentHint.${component.kind}`)}</p>
      </div>
      <span className="badge badge-ghost badge-sm shrink-0 font-mono">{component.license}</span>
      <span className="w-20 shrink-0 text-right text-xs tabular-nums text-base-content/70">
        {component.present ? (
          <span className="inline-flex items-center gap-1 text-success"><Check className="h-3.5 w-3.5" aria-hidden="true" />{t('ocr.install.present')}</span>
        ) : formatBytes(Number(component.bytes), i18n.language)}
      </span>
    </li>
  );
}

/** The download while it runs, and how it ended. */
function Progress({ status }: { status: OcrStatus }) {
  const { t, i18n } = useTranslation('manga');
  const download = status.download!;
  const total = Number(download.total);
  const received = Number(download.received);
  const percent = total > 0 ? Math.min(100, Math.round((received / total) * 100)) : 0;
  const running = isDownloading(status);
  return (
    <div className="space-y-4" aria-live="polite">
      {download.phase === 'failed' ? (
        <div role="alert" className="alert alert-error alert-soft">
          <AlertTriangle className="h-4 w-4" aria-hidden="true" />
          <span>{ocrErrorText(download.error)}</span>
        </div>
      ) : download.phase === 'done' ? (
        <div className="flex items-center gap-3 rounded-box bg-success/10 p-4 text-success">
          <span className="grid h-10 w-10 place-items-center rounded-full bg-success/15"><Check className="h-5 w-5" aria-hidden="true" /></span>
          <div className="text-base-content">
            <p className="font-semibold">{t('ocr.install.doneTitle')}</p>
            <p className="text-sm text-base-content/65">{t('ocr.install.doneBody')}</p>
          </div>
        </div>
      ) : null}
      {download.phase !== 'done' ? (
        <div className="surface-muted space-y-2 p-4">
          <div className="flex items-baseline justify-between gap-3 text-sm">
            <span className="min-w-0 truncate font-medium">
              {download.phase === 'verifying' ? t('ocr.install.verifying') : running ? t('ocr.install.downloading') : download.phase === 'cancelled' ? t('ocr.install.paused') : t('ocr.install.stopped')}
            </span>
            <span className="shrink-0 tabular-nums text-base-content/70">{percent}%</span>
          </div>
          <progress className="progress progress-primary h-2 w-full" value={percent} max={100} aria-label={t('ocr.install.progress')} />
          <div className="flex justify-between gap-3 text-xs tabular-nums text-base-content/55">
            <span className="min-w-0 truncate font-mono">{download.file}</span>
            <span className="shrink-0">{t('ocr.install.of', { received: formatBytes(received, i18n.language), total: formatBytes(total, i18n.language) })}</span>
          </div>
        </div>
      ) : null}
      {running ? <p className="text-xs text-base-content/55">{t('ocr.install.background')}</p> : null}
      {download.phase === 'cancelled' || download.phase === 'failed' ? <p className="text-xs text-base-content/55">{t('ocr.install.resumes')}</p> : null}
    </div>
  );
}

/**
 * Asks before anything is downloaded: what the chosen built-in engine
 * (manga-ocr or meikiocr) needs, how much it is, how much room there is and
 * where it goes. Nothing leaves until the user says so; then the same dialog
 * follows the download.
 */
export function OcrInstallModal() {
  const { t, i18n } = useTranslation(['manga', 'common']);
  const open = useOcrInstallStore((s) => s.open);
  const then = useOcrInstallStore((s) => s.then);
  const close = useOcrInstallStore((s) => s.close);
  const status = useOcrStatus().data;
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const queryClient = useQueryClient();
  const [starting, setStarting] = useState(false);
  const [started, setStarted] = useState(false);
  const ran = useRef(false);

  useEffect(() => {
    if (!open) {
      setStarted(false);
      ran.current = false;
    }
  }, [open]);
  // Installed while the dialog is up: what asked for it goes ahead.
  useEffect(() => {
    if (open && started && status?.download?.phase === 'done' && status.installed && then && !ran.current) {
      ran.current = true;
      then();
    }
  }, [open, started, status, then]);

  if (!status || !settings) return null;
  const meiki = status.engine === 'meiki';
  const downloading = isDownloading(status);
  const showProgress = Boolean(status.download) && (started || downloading);
  const download = Number(status.download_bytes);
  const free = status.free_bytes === null ? null : Number(status.free_bytes);
  const tight = free !== null && free < download * SPACE_MARGIN;

  const chooseVariant = async (variant: OcrVariant) => {
    if (variant === settings.manga_ocr_variant) return;
    await updateSettings({ manga_ocr_variant: variant });
    await queryClient.invalidateQueries({ queryKey: OCR_QUERY });
  };
  const start = async () => {
    setStarting(true);
    try {
      await ipc.mangaOcrInstall();
      setStarted(true);
      await queryClient.invalidateQueries({ queryKey: OCR_QUERY });
    } catch (error) {
      setStarted(true);
      queryClient.setQueryData<OcrStatus>(OCR_QUERY, (current) => current ? { ...current, download: { phase: 'failed', file: '', received: 0, total: 0, error: String(error) } } : current);
    } finally {
      setStarting(false);
    }
  };

  const actions = showProgress ? (
    downloading ? (
      <>
        <Button appearance="ghost" onClick={close}>{t('ocr.install.hide')}</Button>
        <Button appearance="outline" onClick={() => void ipc.mangaOcrCancelInstall()}><X className="h-4 w-4" />{t('ocr.install.cancelDownload')}</Button>
      </>
    ) : status.download?.phase === 'done' ? (
      <Button variant="primary" onClick={close}>{t('ocr.install.close')}</Button>
    ) : (
      <>
        <Button appearance="ghost" onClick={close}>{t('common:cancel')}</Button>
        <Button variant="primary" loading={starting} onClick={() => void start()}>{starting ? null : <Download className="h-4 w-4" />}{t('ocr.install.resume')}</Button>
      </>
    )
  ) : (
    <>
      <Button appearance="ghost" onClick={close}>{t('common:cancel')}</Button>
      <Button variant="primary" loading={starting} disabled={!status.supported || download === 0} onClick={() => void start()}>
        {starting ? null : <Download className="h-4 w-4" />}
        {t('ocr.install.download', { size: formatBytes(download, i18n.language) })}
      </Button>
    </>
  );

  return (
    <Modal open={open} onClose={close} title={t(meiki ? 'ocr.install.titleMeiki' : 'ocr.install.title')} size="lg" actions={actions}>
      {showProgress ? <Progress status={status} /> : !status.supported ? (
        <div role="alert" className="alert alert-warning alert-soft"><AlertTriangle className="h-4 w-4" aria-hidden="true" /><span>{t('ocr.errors.unsupported')}</span></div>
      ) : (
        <div className="space-y-5">
          <p className="text-sm text-base-content/75">{t(meiki ? 'ocr.install.introMeiki' : 'ocr.install.intro')}</p>
          {meiki ? (
            <div className="flex items-start gap-3 rounded-box border border-warning/30 bg-warning/5 p-3 text-sm">
              <Gauge className="mt-0.5 h-4 w-4 shrink-0 text-warning" aria-hidden="true" />
              <div className="space-y-1">
                <p className="font-medium">{t('ocr.install.meikiTradeoffTitle')}</p>
                <p className="text-xs text-base-content/65">{t('ocr.install.meikiTradeoff')}</p>
              </div>
            </div>
          ) : (
            <div role="radiogroup" aria-label={t('ocr.install.variant')} className="grid gap-2 sm:grid-cols-2">
              {VARIANTS.map((variant) => {
                const checked = settings.manga_ocr_variant === variant;
                return (
                  <button
                    key={variant}
                    type="button"
                    role="radio"
                    aria-checked={checked}
                    onClick={() => void chooseVariant(variant)}
                    className={checked
                      ? 'rounded-box border-2 border-primary bg-primary/5 p-3 text-left motion-safe:transition-colors'
                      : 'rounded-box border-2 border-base-300 p-3 text-left motion-safe:transition-colors hover:border-base-content/30'}
                  >
                    <span className="flex items-center justify-between gap-2">
                      <span className="text-sm font-semibold">{t(`ocr.variant.${variant}`)}</span>
                      {checked ? <Check className="h-4 w-4 text-primary" aria-hidden="true" /> : null}
                    </span>
                    <span className="mt-1 block text-xs text-base-content/60">{t(`ocr.variant.${variant}Hint`)}</span>
                  </button>
                );
              })}
            </div>
          )}
          <ul className="surface-muted divide-y divide-base-300/60 overflow-hidden">
            {status.components.map((component) => <ComponentRow key={component.kind} component={component} status={status} />)}
          </ul>
          <dl className="grid gap-3 text-sm sm:grid-cols-2">
            <div className="flex items-center gap-2">
              <Download className="h-4 w-4 text-base-content/50" aria-hidden="true" />
              <dt className="text-base-content/60">{t('ocr.install.total')}</dt>
              <dd className="ml-auto font-semibold tabular-nums">{formatBytes(download, i18n.language)}</dd>
            </div>
            <div className="flex items-center gap-2">
              <HardDrive className="h-4 w-4 text-base-content/50" aria-hidden="true" />
              <dt className="text-base-content/60">{t('ocr.install.free')}</dt>
              <dd className={tight ? 'ml-auto font-semibold tabular-nums text-error' : 'ml-auto font-semibold tabular-nums'}>
                {free === null ? t('ocr.install.unknown') : formatBytes(free, i18n.language)}
              </dd>
            </div>
            <div className="sm:col-span-2">
              <dt className="text-xs text-base-content/55">{t('ocr.install.where')}</dt>
              <dd className="selectable truncate font-mono text-xs text-base-content/75" title={status.dir}>{status.dir}</dd>
            </div>
          </dl>
          {tight ? (
            <div role="alert" className="alert alert-warning alert-soft text-sm"><AlertTriangle className="h-4 w-4" aria-hidden="true" /><span>{t('ocr.install.noRoom')}</span></div>
          ) : null}
          <p className="flex items-start gap-2 text-xs text-base-content/60">
            <ShieldCheck className="mt-0.5 h-4 w-4 shrink-0 text-success" aria-hidden="true" />
            <span>{t(status.accelerator ? `ocr.install.privacy_${status.accelerator}` : 'ocr.install.privacy')}</span>
          </p>
          {meiki ? <p className="text-xs text-base-content/55">{t('ocr.install.meikiLicense')}</p> : null}
        </div>
      )}
    </Modal>
  );
}
