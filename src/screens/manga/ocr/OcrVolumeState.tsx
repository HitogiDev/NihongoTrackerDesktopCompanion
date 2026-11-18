import { useTranslation } from 'react-i18next';
import { Loader2, ScanText, X } from 'lucide-react';
import { Button } from '../../../components/ui/Button';
import { ipc } from '../../../lib/ipc';
import type { MangaVolume, OcrStatus } from '../../../lib/types';
import { MANGA } from '../Cover';
import { processVolume, runOf } from './useOcr';

export type OcrVolumeKind = 'mokuro' | 'done' | 'partial' | 'none';

/** What text a volume has, from the library's point of view. */
export function volumeTextKind(volume: MangaVolume): OcrVolumeKind {
  const pages = Number(volume.page_count);
  const read = Number(volume.ocr_pages);
  if (volume.ocr_state === 'mokuro') return 'mokuro';
  if (pages > 0 && read >= pages) return 'done';
  if (read > 0) return 'partial';
  return 'none';
}

/**
 * The OCR state of a volume in its row: no text, reading x/y (with a way to
 * stop), partial, ready, or mokuro's; and "Read text" when some page has none.
 */
export function OcrVolumeState({ volume, status, disabled }: { volume: MangaVolume; status: OcrStatus | undefined; disabled?: boolean }) {
  const { t } = useTranslation('manga');
  const run = runOf(status, volume.id);
  if (run) {
    const done = Number(run.done) + Number(run.failed);
    const total = Math.max(1, Number(run.total));
    return (
      <div className="flex w-44 shrink-0 items-center gap-2" role="status">
        <div className="min-w-0 flex-1">
          <div className="flex items-baseline justify-between gap-2 text-[11px] tabular-nums text-base-content/60">
            <span className="inline-flex items-center gap-1"><Loader2 className="h-3 w-3 motion-safe:animate-spin" aria-hidden="true" />{t('ocr.state.processing', { done, total: Number(run.total) })}</span>
          </div>
          <progress className="progress progress-primary mt-1 h-1.5 w-full" value={Math.round((done / total) * 100)} max={100} aria-label={t('ocr.volume.progress', { title: volume.title })} />
        </div>
        <Button appearance="ghost" size="xs" shape="square" aria-label={t('ocr.volume.cancel')} title={t('ocr.volume.cancel')} onClick={() => void ipc.mangaOcrCancelVolume(volume.id)}>
          <X className="h-3.5 w-3.5" />
        </Button>
      </div>
    );
  }
  const kind = volumeTextKind(volume);
  if (kind === 'mokuro' || kind === 'done') return null;
  return (
    <Button
      appearance="ghost"
      size="sm"
      disabled={disabled || status?.supported === false}
      title={status?.installed ? t('ocr.volume.processHint') : t('ocr.volume.installHint')}
      onClick={() => void processVolume(volume.id, Boolean(status?.installed))}
    >
      <ScanText className={`h-4 w-4 ${MANGA.color}`} />
      <span className="hidden sm:inline">{t('ocr.volume.process')}</span>
    </Button>
  );
}

/** The short label after the page count: where the text comes from. */
export function OcrVolumeLabel({ volume }: { volume: MangaVolume }) {
  const { t } = useTranslation('manga');
  const kind = volumeTextKind(volume);
  switch (kind) {
    case 'mokuro':
      return <span className={MANGA.color}>{t('detail.textMokuro')}</span>;
    case 'done':
      return <span className={MANGA.color}>{t('ocr.state.done')}</span>;
    case 'partial':
      return <span className="text-base-content/70">{t('ocr.state.partial', { count: Number(volume.ocr_pages), total: Number(volume.page_count) })}</span>;
    default:
      return <>{t('detail.noText')}</>;
  }
}
