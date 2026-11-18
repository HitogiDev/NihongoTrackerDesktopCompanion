import { useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { Check, Download, Trash2 } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import DropdownSelect from '../../components/ui/DropdownSelect';
import { Field, Toggle } from '../../components/ui/Field';
import { ipc } from '../../lib/ipc';
import { formatBytes } from '../../lib/manga';
import { toast } from '../../store/useToastStore';
import { useAppStore } from '../../store/useAppStore';
import type { OcrEngine, OcrVariant, Settings } from '../../lib/types';
import { OcrInstallModal } from '../manga/ocr/OcrInstallModal';
import { OwocrSettings } from '../manga/ocr/OwocrSettings';
import { ExtensionsSettings } from './ExtensionsSettings';
import { isDownloading, OCR_QUERY, ocrErrorText, useOcrInstallStore, useOcrStatus } from '../manga/ocr/useOcr';

const CACHE_SIZES_MB = [512, 1024, 2048, 5120, 10240, 20480];

/** A limit as it was chosen: 2048 MB is "2 GB", not 2.1. */
function limitLabel(mb: number, locale: string): string {
  const format = new Intl.NumberFormat(locale, { maximumFractionDigits: 1 });
  return mb >= 1024 ? `${format.format(mb / 1024)} GB` : `${format.format(mb)} MB`;
}

/** The WebDAV cache: how full it is, its limit, and a way to empty it. */
function WebdavCache({ settings }: { settings: Settings }) {
  const { t, i18n } = useTranslation('settings');
  const updateSettings = useAppStore((s) => s.updateSettings);
  const queryClient = useQueryClient();
  const usage = useQuery({ queryKey: ['manga', 'webdav-cache'], queryFn: ipc.mangaWebdavCacheUsage });
  const [clearing, setClearing] = useState(false);
  const limitMb = Number(settings.manga_webdav_cache_mb);
  const sizes = CACHE_SIZES_MB.includes(limitMb) ? CACHE_SIZES_MB : [...CACHE_SIZES_MB, limitMb].sort((a, b) => a - b);
  const used = Number(usage.data?.cache_bytes ?? 0);
  const limit = limitMb * 1024 * 1024;
  const percent = limit > 0 ? Math.min(100, Math.round((used / limit) * 100)) : 0;

  const clear = async () => {
    setClearing(true);
    try {
      queryClient.setQueryData(['manga', 'webdav-cache'], await ipc.mangaWebdavClearCache());
      toast.success(t('manga.cacheCleared'));
    } catch (error) {
      toast.error(String(error));
    } finally {
      setClearing(false);
    }
  };

  return (
    <div className="p-3">
      <Field label={t('manga.cache')} hint={t('manga.cacheHint')}>
        <div className="flex flex-wrap items-center gap-3">
          <DropdownSelect
            className="select select-sm w-32"
            aria-label={t('manga.cacheLimit')}
            value={limitMb}
            onChange={(event) => void updateSettings({ manga_webdav_cache_mb: Number(event.target.value) })}
          >
            {sizes.map((mb) => <option key={mb} value={mb}>{limitLabel(mb, i18n.language)}</option>)}
          </DropdownSelect>
          <div className="min-w-40 flex-1">
            <div className="flex justify-between text-xs tabular-nums text-base-content/60">
              <span>{t('manga.cacheUsed', { used: formatBytes(used, i18n.language) })}</span>
              <span>{percent}%</span>
            </div>
            <progress className="progress progress-primary mt-1 h-1.5 w-full" value={percent} max={100} aria-label={t('manga.cacheLimit')} />
          </div>
          <Button appearance="outline" size="sm" loading={clearing} disabled={used === 0} onClick={() => void clear()}>
            {clearing ? null : <Trash2 className="h-4 w-4" />}
            {t('manga.cacheClear')}
          </Button>
        </div>
        {usage.data && Number(usage.data.offline_volumes) > 0 ? (
          <p className="mt-1 text-xs text-base-content/60">
            {t('manga.offlineUsage', { count: Number(usage.data.offline_volumes), size: formatBytes(Number(usage.data.offline_bytes), i18n.language) })}
          </p>
        ) : null}
      </Field>
    </div>
  );
}

const OCR_ENGINES: OcrEngine[] = ['builtin', 'meiki', 'owocr'];

/**
 * Which OCR reads pages: manga-ocr or meikiocr inside the app, or owocr,
 * each with its settings. Each option says in a line what it is good at.
 */
function OcrEngineSettings({ settings }: { settings: Settings }) {
  const { t } = useTranslation('settings');
  const updateSettings = useAppStore((s) => s.updateSettings);
  const queryClient = useQueryClient();
  const status = useOcrStatus().data;
  const engine = settings.manga_ocr_engine;
  const installed = (option: OcrEngine) =>
    option === 'builtin' ? status?.manga_ocr_installed : option === 'meiki' ? status?.meiki_installed : false;
  return (
    <>
      <div className="p-3">
        <Field label={t('manga.ocrEngine')} hint={t('manga.ocrEngineHint')}>
          <div className="grid gap-2 sm:grid-cols-3" role="radiogroup" aria-label={t('manga.ocrEngine')}>
            {OCR_ENGINES.map((option) => {
              const checked = engine === option;
              return (
                <button
                  key={option}
                  type="button"
                  role="radio"
                  aria-checked={checked}
                  onClick={async () => {
                    if (checked) return;
                    await updateSettings({ manga_ocr_engine: option });
                    await queryClient.invalidateQueries({ queryKey: OCR_QUERY });
                  }}
                  className={checked
                    ? 'flex flex-col items-start rounded-box border-2 border-primary bg-primary/5 p-3 text-left motion-safe:transition-colors'
                    : 'flex flex-col items-start rounded-box border-2 border-base-300 p-3 text-left motion-safe:transition-colors hover:border-base-content/30'}
                >
                  <span className="flex w-full items-start justify-between gap-2">
                    <span className="min-w-0">
                      <span className="block text-sm font-semibold">{t(`manga.ocrEngines.${option}`)}</span>
                      <span className="block font-mono text-[11px] text-base-content/55">{t(`manga.ocrEngineModel.${option}`)}</span>
                    </span>
                    {checked ? <Check className="h-4 w-4 shrink-0 text-primary" aria-hidden="true" /> : null}
                  </span>
                  <span className="mt-2 block text-xs text-base-content/65">{t(`manga.ocrEngineTagline.${option}`)}</span>
                  {option === 'builtin' ? (
                    <span className="badge badge-primary badge-soft badge-sm mt-2">{t('manga.ocrRecommended')}</span>
                  ) : installed(option) ? (
                    <span className="badge badge-success badge-soft badge-sm mt-2">{t('manga:ocr.state.installed')}</span>
                  ) : null}
                </button>
              );
            })}
          </div>
        </Field>
      </div>
      {engine === 'owocr' ? (
        <div className="p-3">
          <OwocrSettings settings={settings} status={status} />
        </div>
      ) : (
        <OcrSettings settings={settings} />
      )}
    </>
  );
}

/**
 * The chosen built-in engine (manga-ocr or meikiocr): installed or not,
 * where it runs, manga-ocr's variant, delete.
 */
function OcrSettings({ settings }: { settings: Settings }) {
  const { t, i18n } = useTranslation(['settings', 'manga']);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const queryClient = useQueryClient();
  const status = useOcrStatus().data;
  const showInstall = useOcrInstallStore((s) => s.show);
  const [confirming, setConfirming] = useState(false);
  const [deleting, setDeleting] = useState(false);
  if (!status) return null;
  const downloading = isDownloading(status);
  const download = status.download;
  const percent = download && Number(download.total) > 0 ? Math.min(100, Math.round((Number(download.received) / Number(download.total)) * 100)) : 0;
  const where = status.provider ?? status.accelerator;
  const meiki = settings.manga_ocr_engine === 'meiki';

  const remove = async () => {
    setDeleting(true);
    try {
      await ipc.mangaOcrDelete();
      await queryClient.invalidateQueries({ queryKey: OCR_QUERY });
      toast.success(t(meiki ? 'manga.ocrMeikiDeleted' : 'manga.ocrDeleted'));
    } catch (error) {
      toast.error(ocrErrorText(error));
    } finally {
      setDeleting(false);
      setConfirming(false);
    }
  };

  return (
    <div className="p-3">
      <Field label={t(meiki ? 'manga.ocrMeiki' : 'manga.ocr')} hint={t(meiki ? 'manga.ocrMeikiHint' : 'manga.ocrHint')}>
        <div className="flex flex-wrap items-center gap-3">
          <div className="min-w-48 flex-1">
            {downloading ? (
              <>
                <div className="flex justify-between text-xs tabular-nums text-base-content/60">
                  <span>{t('manga:ocr.install.downloading')}</span>
                  <span>{percent}%</span>
                </div>
                <progress className="progress progress-primary mt-1 h-1.5 w-full" value={percent} max={100} aria-label={t('manga:ocr.install.progress')} />
              </>
            ) : status.installed ? (
              <p className="text-sm">
                <span className="badge badge-success badge-soft badge-sm mr-2">{t('manga:ocr.state.installed')}</span>
                <span className="text-base-content/65">
                  {t('manga.ocrInstalled', { size: formatBytes(Number(status.installed_bytes), i18n.language), where: t(`manga:ocr.provider.${where ?? 'cpu'}`) })}
                </span>
              </p>
            ) : (
              <p className="text-sm text-base-content/65">
                {status.supported
                  ? t('manga.ocrMissing', { size: formatBytes(Number(status.download_bytes), i18n.language) })
                  : t('manga:ocr.errors.unsupported')}
              </p>
            )}
            {status.error && !downloading ? <p className="mt-1 text-xs text-error">{ocrErrorText(status.error)}</p> : null}
          </div>
          {meiki ? null : (
            <DropdownSelect
              className="select select-sm w-40"
              aria-label={t('manga.ocrVariant')}
              value={settings.manga_ocr_variant}
              disabled={downloading}
              onChange={async (event) => {
                await updateSettings({ manga_ocr_variant: event.target.value as OcrVariant });
                await queryClient.invalidateQueries({ queryKey: OCR_QUERY });
              }}
            >
              <option value="int8">{t('manga:ocr.variant.int8')}</option>
              <option value="fp16">{t('manga:ocr.variant.fp16')}</option>
            </DropdownSelect>
          )}
          {downloading || !status.installed ? (
            <Button variant="primary" size="sm" disabled={!status.supported} onClick={() => showInstall()}>
              <Download className="h-4 w-4" />
              {downloading ? t('manga.ocrShow') : t('manga.ocrDownload')}
            </Button>
          ) : null}
          {Number(status.installed_bytes) > 0 && !downloading ? (
            confirming ? (
              <span className="flex items-center gap-1">
                <Button variant="error" size="sm" loading={deleting} onClick={() => void remove()}>{deleting ? null : <Trash2 className="h-4 w-4" />}{t('manga.ocrDeleteConfirm')}</Button>
                <Button appearance="ghost" size="sm" onClick={() => setConfirming(false)}>{t('common:cancel')}</Button>
              </span>
            ) : (
              <Button appearance="outline" size="sm" onClick={() => setConfirming(true)}><Trash2 className="h-4 w-4" />{t('manga.ocrDelete')}</Button>
            )
          ) : null}
        </div>
      </Field>
      <OcrInstallModal />
    </div>
  );
}

/** How the manga reader counts what is read. */
export function MangaSection({ settings }: { settings: Settings }) {
  const { t } = useTranslation(['settings', 'common']);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const dwell = Number(settings.manga_page_dwell_secs);
  const idleLog = Number(settings.manga_idle_log_minutes);

  return (
    <div className="surface p-2 divide-y divide-base-300/50">
      <Toggle
        label={t('manga.suggest')}
        description={t('manga.suggestHint')}
        checked={settings.manga_link_suggestions}
        onChange={(v) => void updateSettings({ manga_link_suggestions: v })}
      />
      <Toggle
        label={t('manga.confirm')}
        description={t('manga.confirmHint')}
        checked={settings.manga_confirm_before_log}
        onChange={(v) => void updateSettings({ manga_confirm_before_log: v })}
      />
      <div className="p-3">
        <Field label={t('manga.dwell', { count: dwell })} hint={t('manga.dwellHint')}>
          {(id) => (
            <input
              id={id}
              type="range"
              min={1}
              max={15}
              step={1}
              className="range range-primary range-sm"
              value={dwell}
              onChange={(e) => void updateSettings({ manga_page_dwell_secs: Number(e.target.value) })}
            />
          )}
        </Field>
      </div>
      <div className="p-3">
        <Field label={t('manga.idleLog')} hint={t('manga.idleLogHint')}>
          {(id) => (
            <input
              id={id}
              type="number"
              min={0}
              max={240}
              step={1}
              className="input w-32"
              value={idleLog}
              onChange={(e) => void updateSettings({ manga_idle_log_minutes: Math.max(0, Number(e.target.value)) })}
            />
          )}
        </Field>
      </div>
      <p className="p-3 text-xs text-base-content/60">{t('manga.clock')}</p>
      <Toggle
        label={t('manga.shareProgress')}
        description={t('manga.shareProgressHint')}
        checked={settings.manga_webdav_sync_progress}
        onChange={(v) => void updateSettings({ manga_webdav_sync_progress: v })}
      />
      <WebdavCache settings={settings} />
      <OcrEngineSettings settings={settings} />
      <Toggle
        label={t('manga.ocrAuto')}
        description={t('manga.ocrAutoHint')}
        checked={settings.manga_ocr_auto}
        onChange={(v) => void updateSettings({ manga_ocr_auto: v })}
      />
      <ExtensionsSettings settings={settings} />
    </div>
  );
}
