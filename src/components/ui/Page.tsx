import type { ReactNode } from 'react';
import type { LucideIcon } from 'lucide-react';

export function Page({
  title,
  description,
  actions,
  children,
}: {
  title: string;
  description?: string;
  actions?: ReactNode;
  children: ReactNode;
}) {
  return (
    // A window toolbar rather than a web page header: compact, opaque, and the
    // content uses the whole width of the window instead of a centred column.
    <div className="min-h-full bg-base-300">
      <header className="sticky top-0 z-20 border-b border-base-300/60 bg-base-100">
        <div className="flex min-h-14 items-center gap-4 px-6 py-2.5">
          <div className="min-w-0 flex-1">
            <h1 className="truncate text-base font-semibold leading-tight">{title}</h1>
            {description ? (
              <p className="truncate text-xs text-base-content/60 mt-0.5" title={description}>{description}</p>
            ) : null}
          </div>
          {actions ? <div className="flex items-center gap-2">{actions}</div> : null}
        </div>
      </header>
      <div className="space-y-6 px-6 py-5">{children}</div>
    </div>
  );
}

export function SectionTitle({ children, aside }: { children: ReactNode; aside?: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-3">
      <h2 className="text-[11px] uppercase tracking-[0.2em] text-base-content/60">{children}</h2>
      {aside}
    </div>
  );
}

export function StatTile({
  label,
  value,
  hint,
  icon: Icon,
  progress,
  compact = false,
  accent = 'text-base-content',
}: {
  label: string;
  value: ReactNode;
  hint?: ReactNode;
  icon?: LucideIcon;
  progress?: number | null;
  compact?: boolean;
  accent?: string;
}) {
  return (
    <div className={compact ? 'min-w-0 p-4 sm:p-5' : 'surface min-w-0 p-4 sm:p-5'}>
      <div className="flex items-center gap-2.5">
        {Icon ? <span className="grid h-8 w-8 shrink-0 place-items-center rounded-field bg-base-200 text-base-content/70"><Icon className="h-4 w-4" aria-hidden="true" /></span> : null}
        <div className="min-w-0 text-sm font-medium text-base-content/75">{label}</div>
      </div>
      <div className={`mt-3 min-w-0 font-bold leading-tight tabular-nums ${compact ? 'text-xl' : 'text-3xl'} ${accent}`}>{value}</div>
      {hint ? <div className="mt-1.5 truncate text-xs text-base-content/70" title={typeof hint === 'string' ? hint : undefined}>{hint}</div> : null}
      {progress !== undefined && progress !== null ? <progress className="progress progress-primary mt-3 block h-1.5 w-full" value={progress} max={100} aria-label={label} /> : null}
    </div>
  );
}
