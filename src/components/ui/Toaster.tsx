import { useLayoutEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { AlertTriangle, CheckCircle2, Info, X, XCircle, type LucideIcon } from 'lucide-react';
import { useToastStore, type ToastKind } from '../../store/useToastStore';

// Never interpolate class names: Tailwind v4 scans source text.
const ALERT: Record<ToastKind, string> = {
  success: 'alert-success',
  error: 'alert-error',
  info: 'alert-info',
  warning: 'alert-warning',
};

const ICON: Record<ToastKind, LucideIcon> = {
  success: CheckCircle2,
  error: XCircle,
  info: Info,
  warning: AlertTriangle,
};

/**
 * The toasts of this window. It is a manual popover so it sits in the top
 * layer: a modal opened with `showModal()` would otherwise cover it, and a
 * save that fails inside a modal is exactly where the error must be seen.
 */
export function Toaster() {
  const { t } = useTranslation('common');
  const toasts = useToastStore((s) => s.toasts);
  const dismiss = useToastStore((s) => s.dismiss);
  const ref = useRef<HTMLDivElement>(null);
  const newest = toasts.length > 0 ? toasts[toasts.length - 1].id : null;

  useLayoutEffect(() => {
    const element = ref.current;
    if (!element || typeof element.showPopover !== 'function') return;
    try {
      // Hide and show again so a new toast goes above a modal opened after
      // the previous one: the top layer is ordered by when things were shown.
      if (element.matches(':popover-open')) element.hidePopover();
      if (newest !== null) element.showPopover();
    } catch {
      // An engine without popovers still renders it as a fixed element.
    }
  }, [newest]);

  return (
    <div
      ref={ref}
      popover="manual"
      aria-live="polite"
      className="toast toast-end toast-bottom z-50 m-0 overflow-visible border-0 bg-transparent p-0"
    >
      {toasts.map((item) => {
        const Icon = ICON[item.kind];
        return (
          <div
            key={item.id}
            role={item.kind === 'error' ? 'alert' : 'status'}
            className={`alert ${ALERT[item.kind]} max-w-sm py-2.5 pr-2 shadow-lg`}
          >
            <Icon className="h-4 w-4 shrink-0" />
            <span className="selectable min-w-0 whitespace-pre-line break-words text-sm">{item.message}</span>
            <button
              type="button"
              className="btn btn-ghost btn-xs btn-circle"
              aria-label={t('close')}
              onClick={() => dismiss(item.id)}
            >
              <X className="h-3.5 w-3.5" />
            </button>
          </div>
        );
      })}
    </div>
  );
}
