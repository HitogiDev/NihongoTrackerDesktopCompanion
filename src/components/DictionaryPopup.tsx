import { useEffect, useMemo, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { ArrowLeft, BookOpen } from 'lucide-react';
import {
  DICTIONARY_SCOPE_CLASS,
  StructuredContent,
  scopeDictionaryCss,
  useLookupHistory,
  type Glossary,
  type LookupRequest,
} from '@nihongotracker/yomitan-content';
import { Button } from './ui/Button';

export interface DictionaryEntry {
  expression: string;
  reading: string;
  /** The deinflection chain, when the term was reached through one. */
  process?: string[];
  glossaries: { dictName: string; glossary: Glossary }[];
}

export interface DictionaryPopupProps {
  /** What is being shown. `undefined` while the lookup is in flight. */
  entries: DictionaryEntry[] | undefined;
  loading?: boolean;
  /** Asked for a term. Called for the first lookup and for every cross-reference. */
  onLookup: (request: LookupRequest) => void;
  /** The dictionary's own `styles.css`, if it shipped one. */
  dictionaryCss?: string;
  initial?: LookupRequest;
}

/**
 * The reader's dictionary popup.
 *
 * The rendering, the link policy and the stylesheet scoping all come from the
 * shared package, so this and the web frontend show the same thing. What lives
 * here is the chrome: the header, the back control and its label, because every
 * visible string in this app goes through its own i18n bundle and a shared
 * package has no business shipping English.
 */
export function DictionaryPopup({
  entries,
  loading,
  onLookup,
  dictionaryCss,
  initial,
}: DictionaryPopupProps) {
  const { t } = useTranslation('dictionary');
  const history = useLookupHistory(initial);
  const scrollRef = useRef<HTMLDivElement>(null);

  // The consumer owns the data fetch; the popup only says what it wants shown.
  const current = history.current;
  useEffect(() => {
    if (current) onLookup(current);
  }, [current, onLookup]);

  // A cross-reference replaces the content, so the reader should be back at the
  // top of it rather than halfway down the previous entry.
  useEffect(() => {
    scrollRef.current?.scrollTo({ top: 0 });
  }, [current]);

  const scopedCss = useMemo(
    () => (dictionaryCss ? scopeDictionaryCss(dictionaryCss, DICTIONARY_SCOPE_CLASS).css : ''),
    [dictionaryCss]
  );

  return (
    <div className="surface-raised rounded-box shadow-lg w-full max-w-md overflow-hidden">
      {/* Scoped to the popup, never injected globally: it comes out of a zip
          the user imported. */}
      {scopedCss ? <style>{scopedCss}</style> : null}

      <header className="flex items-center gap-2 px-3 py-2 border-b border-base-300">
        {history.canGoBack ? (
          <Button
            appearance="ghost"
            size="sm"
            shape="circle"
            aria-label={t('back')}
            onClick={history.back}
          >
            <ArrowLeft className="w-4 h-4" />
          </Button>
        ) : (
          <BookOpen className="w-4 h-4 text-base-content/50" />
        )}
        <span className="min-w-0 flex-1 truncate text-sm font-medium">
          {current?.query ?? ''}
        </span>
        {history.depth > 0 ? (
          <span className="badge badge-ghost badge-sm">{t('depth', { count: history.depth })}</span>
        ) : null}
      </header>

      <div ref={scrollRef} className="max-h-96 overflow-y-auto p-3 space-y-4">
        {loading ? <p className="text-sm text-base-content/60">{t('loading')}</p> : null}

        {!loading && entries?.length === 0 ? (
          <p className="text-sm text-base-content/60">{t('noResults', { term: current?.query })}</p>
        ) : null}

        {entries?.map((entry) => (
          <article key={`${entry.expression}-${entry.reading}`} className="space-y-1">
            <h3 className="font-bold">
              {entry.expression}
              {entry.reading && entry.reading !== entry.expression ? (
                <span className="text-base-content/60 font-normal ml-1">【{entry.reading}】</span>
              ) : null}
            </h3>
            {entry.process?.length ? (
              <p className="text-xs text-base-content/50">{entry.process.join(' → ')}</p>
            ) : null}
            {entry.glossaries.map((gloss, index) => (
              <StructuredContent
                key={`${gloss.dictName}-${index}`}
                glossary={gloss.glossary}
                onLookup={history.push}
                className="text-sm"
              />
            ))}
          </article>
        ))}
      </div>
    </div>
  );
}
