import { useEffect, useId, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { ListFilter } from 'lucide-react';
import DropdownSelect from '../../components/ui/DropdownSelect';
import { formatCount } from '../../lib/manga';
import type { MangaSeries } from '../../lib/types';
import { activeFilters, DEFAULT_VIEW, LINK_FILTERS, optionCounts, SORTS, STATUS_FILTERS, TEXT_FILTERS, type LibraryView } from './libraryView';

type FilterKey = 'text' | 'link' | 'status';
const GROUPS: { key: FilterKey; options: readonly string[] }[] = [
  { key: 'status', options: STATUS_FILTERS },
  { key: 'text', options: TEXT_FILTERS },
  { key: 'link', options: LINK_FILTERS },
];

/** Sort and filters for the library grid, next to the search. */
export function LibraryToolbar({ series, view, onChange, suggested }: {
  /** The series the tab and search leave, before these filters. */
  series: MangaSeries[];
  view: LibraryView;
  onChange: (view: LibraryView) => void;
  suggested: ReadonlySet<string>;
}) {
  const { t } = useTranslation('manga');
  return (
    <div className="flex items-center gap-2">
      <DropdownSelect
        className="select select-sm w-48"
        aria-label={t('view.sortLabel')}
        title={t('view.sortLabel')}
        value={view.sort}
        onChange={(event) => onChange({ ...view, sort: event.target.value as LibraryView['sort'] })}
      >
        {SORTS.map((sort) => <option key={sort} value={sort}>{t(`view.sort.${sort}`)}</option>)}
      </DropdownSelect>
      <FilterPopover series={series} view={view} onChange={onChange} suggested={suggested} />
    </div>
  );
}

function FilterPopover({ series, view, onChange, suggested }: {
  series: MangaSeries[];
  view: LibraryView;
  onChange: (view: LibraryView) => void;
  suggested: ReadonlySet<string>;
}) {
  const { t, i18n } = useTranslation('manga');
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const panelId = useId();
  const active = activeFilters(view);
  // Only while open: a thousand series, a dozen passes.
  const counts = useMemo(
    () => (open ? Object.fromEntries(GROUPS.map(({ key, options }) => [key, optionCounts(series, view, key, options as never, suggested)])) : null),
    [open, series, view, suggested],
  ) as Record<FilterKey, Record<string, number>> | null;

  useEffect(() => {
    if (!open) return;
    panel.current?.querySelector<HTMLInputElement>('input:checked')?.focus();
    const onDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!panel.current?.contains(target) && !button.current?.contains(target)) setOpen(false);
    };
    document.addEventListener('pointerdown', onDown);
    return () => document.removeEventListener('pointerdown', onDown);
  }, [open]);

  return (
    <div className="relative">
      <button
        ref={button}
        type="button"
        className={active > 0 || open ? 'btn btn-sm btn-soft btn-primary gap-1.5' : 'btn btn-sm btn-ghost gap-1.5'}
        aria-expanded={open}
        aria-controls={open ? panelId : undefined}
        aria-haspopup="dialog"
        onClick={() => setOpen((value) => !value)}
      >
        <ListFilter className="h-4 w-4" aria-hidden="true" />
        {t('view.filters')}
        {active > 0 ? <span className="badge badge-primary badge-xs tabular-nums" aria-label={t('view.activeCount', { count: active })}>{active}</span> : null}
      </button>
      {open ? (
        <div
          ref={panel}
          id={panelId}
          role="dialog"
          aria-label={t('view.filters')}
          className="surface-raised manga-menu-in absolute right-0 top-full z-30 mt-2 w-[22rem] space-y-4 p-4"
          onKeyDown={(event) => {
            if (event.key === 'Escape') {
              event.preventDefault();
              event.stopPropagation();
              setOpen(false);
              button.current?.focus();
            }
          }}
        >
          {GROUPS.map(({ key, options }) => (
            <fieldset key={key} className="space-y-1.5">
              <legend className="mb-1.5 text-[11px] font-medium uppercase tracking-[0.14em] text-base-content/55">{t(`view.${key}.label`)}</legend>
              <div className="grid grid-cols-2 gap-1.5">
                {options.map((option) => {
                  const checked = view[key] === option;
                  return (
                    <label
                      key={option}
                      className={checked
                        ? 'flex cursor-pointer items-center justify-between gap-2 rounded-field border border-primary/40 bg-primary/10 px-2.5 py-1.5 text-sm font-medium text-base-content has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-offset-2 has-[:focus-visible]:outline-primary'
                        : 'flex cursor-pointer items-center justify-between gap-2 rounded-field border border-base-300 px-2.5 py-1.5 text-sm text-base-content/80 motion-safe:transition-colors hover:bg-base-200 has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-offset-2 has-[:focus-visible]:outline-primary'}
                    >
                      <input
                        type="radio"
                        className="sr-only"
                        name={`${panelId}-${key}`}
                        value={option}
                        checked={checked}
                        onChange={() => onChange({ ...view, [key]: option })}
                      />
                      <span className="truncate">{t(`view.${key}.${option}`)}</span>
                      {counts ? <span className="shrink-0 text-xs tabular-nums text-base-content/45">{formatCount(counts[key][option] ?? 0, i18n.language)}</span> : null}
                    </label>
                  );
                })}
              </div>
            </fieldset>
          ))}
          <div className="flex justify-end border-t border-base-300/60 pt-3">
            <button
              type="button"
              className="btn btn-ghost btn-sm"
              disabled={active === 0}
              onClick={() => onChange({ ...DEFAULT_VIEW, sort: view.sort })}
            >
              {t('view.clear')}
            </button>
          </div>
        </div>
      ) : null}
    </div>
  );
}
