import { useTranslation } from 'react-i18next';
import type { NativeGameHookStatus } from '../lib/types';
import { Button } from './ui/Button';

export function NativeHookChoices({ status, title, selected, onSelect, enable, trackingRequired = false }: {
  status: NativeGameHookStatus | null;
  title?: string;
  selected: string | null;
  onSelect: (code: string) => void;
  enable?: { busy: boolean; disabled: boolean; onClick: () => void };
  trackingRequired?: boolean;
}) {
  const { t } = useTranslation('texthooker');
  return <>
    <p className="mb-4 text-sm text-base-content/70">{t('hookSelection.description')}</p>
    <div className="surface-muted mb-4 space-y-2 p-3 text-sm" role="status">
      {(title || status?.exe) && <p className="font-medium">{title || status?.exe}</p>}
      <p>{t(`hookSelection.status.${status?.phase ?? 'inactive'}`)}</p>
      {status?.message && <p className="break-words text-error">{status.message}</p>}
      {!status && !title && <p>{t('hookSelection.noGame')}</p>}
      {enable && <Button size="sm" loading={enable.busy} disabled={enable.disabled} onClick={enable.onClick}>{t('hookSelection.enable')}</Button>}
      {trackingRequired && <p>{t('hookSelection.trackingRequired')}</p>}
      {status && status.hooks.length === 0 && <p>{t('hookSelection.waiting')}</p>}
      {status?.selected_hook_code && <p className="break-all">{t('hookSelection.saved', { code: status.selected_hook_code.split('\u001f')[0] })}</p>}
    </div>
    <div className="space-y-2">
      {(status?.hooks ?? []).map((hook) => <label key={hook.selection_key}
        className={selected === hook.selection_key
          ? 'flex cursor-pointer items-start gap-3 rounded-box border border-primary bg-primary/10 p-3'
          : 'flex cursor-pointer items-start gap-3 rounded-box border border-base-300 p-3 hover:bg-base-200'}>
        <input type="radio" name="native-hook-selection" className="radio radio-primary mt-0.5" checked={selected === hook.selection_key} onChange={() => onSelect(hook.selection_key)} />
        <span className="min-w-0 flex-1">
          <span className="block font-medium">{hook.name || t('hookSelection.unnamed')}</span>
          {hook.sample && <span className="mt-1 block truncate text-sm text-base-content/70" title={hook.sample}>{hook.sample}</span>}
          <code className="mt-1 block truncate text-xs text-base-content/50" title={hook.hook_code}>{hook.hook_code}</code>
        </span>
      </label>)}
    </div>
  </>;
}
