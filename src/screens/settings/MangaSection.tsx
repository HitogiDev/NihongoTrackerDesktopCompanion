import { useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { Trash2 } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import DropdownSelect from '../../components/ui/DropdownSelect';
import { Field, Toggle } from '../../components/ui/Field';
import { ipc } from '../../lib/ipc';
import { formatBytes } from '../../lib/manga';
import { toast } from '../../store/useToastStore';
import { useAppStore } from '../../store/useAppStore';
import type { Settings } from '../../lib/types';

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

/** How the manga reader counts what is read. */
export function MangaSection({ settings }: { settings: Settings }) {
  const { t } = useTranslation(['settings', 'common']);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const dwell = Number(settings.manga_page_dwell_secs);
  const idleLog = Number(settings.manga_idle_log_minutes);

  return (
    <div className="surface p-2 divide-y divide-base-300/50">
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
    </div>
  );
}
