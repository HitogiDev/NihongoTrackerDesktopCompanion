import { useTranslation } from 'react-i18next';
import { Check, ScanText, SquareDashedMousePointer, Trash2, Undo2, X } from 'lucide-react';
import { Button } from '../../../components/ui/Button';
import type { EditedBlock } from '../../../lib/types';

/**
 * The panel of the text editor: the selected block's text and direction,
 * reading it again, deleting it; and saving or discarding the page's changes.
 */
export function BlockEditor({ block, index, count, dirty, saving, recognizing, canRecognize, onChange, onDelete, onRecognize, onDiscard, onDone }: {
  block: EditedBlock | null;
  index: number | null;
  count: number;
  dirty: boolean;
  saving: boolean;
  recognizing: boolean;
  /** The OCR is installed: a box can be read again. */
  canRecognize: boolean;
  onChange: (patch: Partial<EditedBlock>) => void;
  onDelete: () => void;
  onRecognize: () => void;
  onDiscard: () => void;
  onDone: () => void;
}) {
  const { t } = useTranslation('mangaReader');
  return (
    <div
      className="surface-raised pointer-events-auto absolute left-3 top-14 z-40 w-80 space-y-4 p-4 text-base-content"
      role="dialog"
      aria-label={t('edit.title')}
      data-reader-chrome
      onClick={(event) => event.stopPropagation()}
    >
      <div className="flex items-center justify-between gap-2">
        <h2 className="text-sm font-semibold">{t('edit.title')}</h2>
        <span className="text-xs tabular-nums text-base-content/55">{t('edit.count', { count })}</span>
      </div>
      {block && index !== null ? (
        <div className="space-y-3">
          <label className="block">
            <span className="mb-1.5 flex items-center justify-between text-xs font-medium text-base-content/60">
              {t('edit.blockTitle', { index: index + 1 })}
              {recognizing ? <span className="inline-flex items-center gap-1 text-primary"><span className="loading loading-spinner loading-xs" aria-hidden="true" />{t('edit.reading')}</span> : null}
            </span>
            <textarea
              lang="ja"
              rows={4}
              autoFocus
              className="textarea w-full font-[family-name:var(--font-jp)] text-base leading-relaxed"
              value={block.text}
              placeholder={t('edit.placeholder')}
              onChange={(event) => onChange({ text: event.target.value })}
            />
            <span className="mt-1 block text-[11px] text-base-content/50">{t('edit.linesHint')}</span>
          </label>
          <label className="flex cursor-default items-center justify-between gap-3 text-sm">
            <span>{t('edit.vertical')}</span>
            <input type="checkbox" className="toggle toggle-sm toggle-primary" checked={block.vertical} onChange={(event) => onChange({ vertical: event.target.checked })} />
          </label>
          <div className="flex flex-wrap gap-2">
            {canRecognize ? (
              <Button appearance="outline" size="sm" loading={recognizing} onClick={onRecognize}>
                {recognizing ? null : <ScanText className="h-4 w-4" />}
                {t('edit.recognize')}
              </Button>
            ) : null}
            <Button appearance="outline" variant="error" size="sm" onClick={onDelete}>
              <Trash2 className="h-4 w-4" />
              {t('edit.delete')}
            </Button>
          </div>
        </div>
      ) : (
        <p className="flex items-start gap-2 rounded-field bg-base-200/70 p-3 text-xs text-base-content/70">
          <SquareDashedMousePointer className="mt-0.5 h-4 w-4 shrink-0" aria-hidden="true" />
          {t('edit.hint')}
        </p>
      )}
      <div className="flex items-center justify-between gap-2 border-t border-base-300/60 pt-3">
        <Button appearance="ghost" size="sm" disabled={!dirty || saving} onClick={onDiscard}>
          <Undo2 className="h-4 w-4" />
          {t('edit.discard')}
        </Button>
        <Button variant="primary" size="sm" loading={saving} onClick={onDone}>
          {saving ? null : dirty ? <Check className="h-4 w-4" /> : <X className="h-4 w-4" />}
          {dirty ? t('edit.save') : t('edit.close')}
        </Button>
      </div>
    </div>
  );
}
