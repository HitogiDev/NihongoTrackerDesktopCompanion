import { useEffect, useId, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { ArrowDownUp, Check, ChevronDown, ListFilter } from 'lucide-react';
import { formatCount } from '../../lib/manga';
import type { MangaSeries } from '../../lib/types';
import { activeFilters, DEFAULT_VIEW, LINK_FILTERS, optionCounts, SORTS, STATUS_FILTERS, TEXT_FILTERS, type LibraryView } from './libraryView';

type FilterKey = 'text' | 'link' | 'status';
const GROUPS: { key: FilterKey; options: readonly string[] }[] = [
  { key: 'status', options: STATUS_FILTERS },
  { key: 'text', options: TEXT_FILTERS },
  { key: 'link', options: LINK_FILTERS },
];

/** Sort and filters for the library grid, in its header. */
export function LibraryToolbar({ series, view, onChange, suggested }: {
  /** The series the tab and search leave, before these filters. */
  series: MangaSeries[];
  view: LibraryView;
  onChange: (view: LibraryView) => void;
  suggested: ReadonlySet<string>;
}) {
  return (
    <div className="flex items-center gap-1">
      <SortMenu value={view.sort} onChange={(sort) => onChange({ ...view, sort })} />
      <FilterPopover series={series} view={view} onChange={onChange} suggested={suggested} />
    </div>
  );
}

/** The order of the grid: a menu of radio items (arrows, Home/End, Esc). */
function SortMenu({ value, onChange }: { value: LibraryView['sort']; onChange: (sort: LibraryView['sort']) => void }) {
  const { t } = useTranslation('manga');
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const menuId = useId();
  const items = () => Array.from(panel.current?.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]') ?? []);
  useEffect(() => {
    if (!open) return;
    (items().find((item) => item.getAttribute('aria-checked') === 'true') ?? items()[0])?.focus();
    const onDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!panel.current?.contains(target) && !button.current?.contains(target)) setOpen(false);
    };
    document.addEventListener('pointerdown', onDown);
    return () => document.removeEventListener('pointerdown', onDown);
  }, [open]);
  const close = () => { setOpen(false); button.current?.focus(); };
  return (
    <div className="relative">
      <button
        ref={button}
        type="button"
        className={open ? 'btn btn-ghost btn-sm btn-active gap-1.5' : 'btn btn-ghost btn-sm gap-1.5'}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        aria-label={`${t('view.sortLabel')}: ${t(`view.sort.${value}`)}`}
        title={t('view.sortLabel')}
        onClick={() => setOpen((current) => !current)}
      >
        <ArrowDownUp className="h-4 w-4" aria-hidden="true" />
        {t(`view.sort.${value}`)}
        <ChevronDown className="h-3.5 w-3.5 opacity-60" aria-hidden="true" />
      </button>
      {open ? (
        <div
          ref={panel}
          id={menuId}
          role="menu"
          aria-label={t('view.sortLabel')}
          className="surface-raised manga-menu-in absolute right-0 top-full z-30 mt-2 w-56 p-1.5"
          onKeyDown={(event) => {
            const list = items();
            const index = list.indexOf(document.activeElement as HTMLButtonElement);
            const next = event.key === 'ArrowDown' ? (index + 1) % list.length
              : event.key === 'ArrowUp' ? (index - 1 + list.length) % list.length
              : event.key === 'Home' ? 0 : event.key === 'End' ? list.length - 1 : null;
            if (next !== null) {
              event.preventDefault();
              list[next]?.focus();
            } else if (event.key === 'Escape') {
              event.preventDefault();
              event.stopPropagation();
              close();
            } else if (event.key === 'Tab') {
              setOpen(false);
            }
          }}
        >
          {SORTS.map((sort) => (
            <button
              key={sort}
              type="button"
              role="menuitemradio"
              aria-checked={sort === value}
              tabIndex={-1}
              onClick={() => { onChange(sort); close(); }}
              className="flex w-full items-center gap-3 rounded-field px-2.5 py-2 text-left text-sm outline-none motion-safe:transition-colors hover:bg-base-200 focus-visible:bg-base-200"
            >
              <span className="min-w-0 flex-1 truncate">{t(`view.sort.${sort}`)}</span>
              {sort === value ? <Check className="h-4 w-4 shrink-0 text-primary" aria-hidden="true" /> : null}
            </button>
          ))}
        </div>
      ) : null}
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
