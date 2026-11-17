import { useMemo, useState, type KeyboardEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { Search } from 'lucide-react';
import { matchesSearch, normalizeSearch } from '../../lib/search';
import { SEARCH_INDEX, labelPrefix, type SectionId } from './sections';

const MAX_RESULTS = 8;

interface Result {
  section: SectionId;
  /** As shown, placeholders folded to "…". */
  label: string;
  /** What the row starts with on the page, to find and ring it. */
  prefix: string;
}

/** The raw string, placeholders included: `t()` without values would mangle it. */
function rawText(i18n: ReturnType<typeof useTranslation>['i18n'], key: string): string {
  const [ns, path] = key.split(':');
  const value = i18n.getResource(i18n.language, ns, path) ?? i18n.getResource('en', ns, path);
  return typeof value === 'string' ? value : '';
}

export function SettingsSearch({ onPick }: { onPick: (section: SectionId, prefix: string) => void }) {
  const { t, i18n } = useTranslation('settings');
  const [query, setQuery] = useState('');
  const [active, setActive] = useState(0);

  const results = useMemo<Result[]>(() => {
    if (!query.trim()) return [];
    const wanted = normalizeSearch(query.trim());
    return SEARCH_INDEX.flatMap((entry) => {
      const label = rawText(i18n, entry.label);
      const hint = entry.hint ? rawText(i18n, entry.hint) : '';
      const sectionName = t(`sections.${entry.section}`);
      if (!label || !matchesSearch(`${label} ${hint} ${sectionName}`, query)) return [];
      // A word of the label starting with the query beats a hit in a hint.
      const words = normalizeSearch(label).split(/[^\p{L}\p{N}]+/u);
      const rank = words.some((word) => word.startsWith(wanted)) ? 0 : matchesSearch(label, query) ? 1 : 2;
      return [{ rank, section: entry.section, label: label.replace(/\{\{[^}]*\}\}/g, '…'), prefix: labelPrefix(label) }];
    })
      .sort((a, b) => a.rank - b.rank)
      .slice(0, MAX_RESULTS)
      .map(({ rank: _rank, ...result }) => result);
    // i18n.language: the index is read in the current language.
  }, [query, i18n, i18n.language, t]);
  const selected = Math.min(active, Math.max(0, results.length - 1));

  const pick = (result: Result | undefined) => {
    if (!result) return;
    onPick(result.section, result.prefix);
    setQuery('');
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Escape' && query) {
      event.preventDefault();
      setQuery('');
    } else if ((event.key === 'ArrowDown' || event.key === 'ArrowUp') && results.length > 0) {
      event.preventDefault();
      const step = event.key === 'ArrowDown' ? 1 : -1;
      setActive((selected + step + results.length) % results.length);
    } else if (event.key === 'Enter') {
      event.preventDefault();
      pick(results[selected]);
    }
  };

  const open = query.trim().length > 0;
  return (
    <div className="relative">
      <label className="input input-sm w-full">
        <Search className="h-4 w-4 opacity-60" />
        <input
          type="search"
          role="combobox"
          aria-expanded={open}
          aria-controls="settings-search-results"
          aria-activedescendant={open && results[selected] ? `settings-result-${selected}` : undefined}
          aria-label={t('search.label')}
          placeholder={t('search.placeholder')}
          value={query}
          onChange={(event) => {
            setQuery(event.target.value);
            setActive(0);
          }}
          onKeyDown={onKeyDown}
        />
      </label>
      {open ? (
        <ul
          id="settings-search-results"
          role="listbox"
          aria-label={t('search.label')}
          className="surface-raised absolute inset-x-0 top-full z-30 mt-1 max-h-80 overflow-y-auto p-1 sm:w-72"
        >
          {results.length === 0 ? (
            <li className="px-3 py-3 text-sm text-base-content/60">{t('search.empty')}</li>
          ) : results.map((result, index) => (
            <li
              key={`${result.section}:${result.label}`}
              id={`settings-result-${index}`}
              role="option"
              aria-selected={index === selected}
              className={index === selected
                ? 'cursor-pointer rounded-field bg-base-200 px-3 py-2'
                : 'cursor-pointer rounded-field px-3 py-2'}
              onMouseMove={() => { if (index !== selected) setActive(index); }}
              // Before the input's blur, so the click is not lost.
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => pick(result)}
            >
              <span className="block truncate text-sm">{result.label}</span>
              <span className="block truncate text-xs text-base-content/55">{t(`sections.${result.section}`)}</span>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}
