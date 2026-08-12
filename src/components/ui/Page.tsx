import type { ReactNode } from 'react';

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
    <div className="min-h-full">
      <header className="sticky top-0 z-20 bg-base-200/85 backdrop-blur border-b border-base-300/60">
        <div className="px-6 py-4 flex items-start gap-4">
          <div className="min-w-0 flex-1">
            <h1 className="text-xl font-bold leading-tight">{title}</h1>
            {description ? (
              <p className="text-sm text-base-content/60 mt-0.5">{description}</p>
            ) : null}
          </div>
          {actions ? <div className="flex items-center gap-2">{actions}</div> : null}
        </div>
      </header>
      <div className="px-6 py-5 space-y-5">{children}</div>
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
  accent = 'text-primary',
}: {
  label: string;
  value: ReactNode;
  hint?: ReactNode;
  accent?: string;
}) {
  return (
    <div className="p-4 rounded-box bg-base-200/60 border border-base-300">
      <div className="text-[11px] uppercase tracking-wide text-base-content/60">{label}</div>
      <div className={`text-2xl font-bold mt-1 ${accent}`}>{value}</div>
      {hint ? <div className="text-xs text-base-content/60 mt-1">{hint}</div> : null}
    </div>
  );
}
