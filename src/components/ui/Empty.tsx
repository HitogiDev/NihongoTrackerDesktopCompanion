import type { LucideIcon } from 'lucide-react';
import { AlertTriangle, RefreshCw } from 'lucide-react';
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { Spinner } from './Spinner';

export function EmptyState({
  icon: Icon,
  title,
  description,
  action,
  compact = false,
}: {
  icon: LucideIcon;
  title: string;
  description?: string;
  action?: ReactNode;
  /** For a section inside a screen rather than the whole screen. */
  compact?: boolean;
}) {
  return (
    <div className={compact
      ? 'flex flex-col items-center justify-center text-center py-8 px-6'
      : 'flex flex-col items-center justify-center text-center py-14 px-6'}
    >
      <Icon className={compact ? 'w-8 h-8 text-base-content/25' : 'w-12 h-12 text-base-content/25'} />
      <h3 className={compact ? 'mt-3 text-sm font-semibold text-base-content' : 'mt-4 font-semibold text-base-content'}>{title}</h3>
      {description ? (
        <p className="mt-1 text-sm text-base-content/60 max-w-md">{description}</p>
      ) : null}
      {action ? <div className="mt-5">{action}</div> : null}
    </div>
  );
}

/**
 * A query that failed: what failed, the reason when there is one, and a way
 * to ask again. Every screen uses it, so a failure never looks like "empty".
 */
export function ErrorState({
  title,
  error,
  onRetry,
  retrying = false,
  compact = false,
}: {
  title: string;
  error?: unknown;
  onRetry?: () => void;
  retrying?: boolean;
  compact?: boolean;
}) {
  const { t } = useTranslation('common');
  const detail = error === undefined || error === null ? '' : String(error);
  return (
    <div
      role="alert"
      className={compact
        ? 'flex flex-col items-center justify-center text-center py-8 px-6'
        : 'flex flex-col items-center justify-center text-center py-14 px-6'}
    >
      <AlertTriangle className={compact ? 'w-8 h-8 text-error/70' : 'w-12 h-12 text-error/70'} />
      <h3 className={compact ? 'mt-3 text-sm font-semibold text-base-content' : 'mt-4 font-semibold text-base-content'}>{title}</h3>
      {detail ? (
        <p className="selectable mt-1 max-w-md break-words text-xs text-base-content/60">{detail}</p>
      ) : null}
      {onRetry ? (
        <button type="button" className="btn btn-outline btn-sm mt-4" disabled={retrying} onClick={onRetry}>
          {retrying ? <Spinner size="xs" /> : <RefreshCw className="h-4 w-4" />}
          {t('retry')}
        </button>
      ) : null}
    </div>
  );
}

export function Skeleton({ className = '' }: { className?: string }) {
  return <div className={`skeleton ${className}`} />;
}

/** Placeholder rows while a list loads, the same height as a list row. */
export function ListSkeleton({ rows = 3, className = 'h-14' }: { rows?: number; className?: string }) {
  const { t } = useTranslation('common');
  return (
    <div className="space-y-2" aria-busy="true" aria-label={t('loading')}>
      {Array.from({ length: rows }, (_, index) => <Skeleton key={index} className={`w-full rounded-field ${className}`} />)}
    </div>
  );
}
