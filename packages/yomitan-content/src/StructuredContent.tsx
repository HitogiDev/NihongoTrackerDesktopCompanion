import { useMemo, type ReactNode } from 'react';
import { renderStructuredContent } from './render';
import type { Glossary, LookupRequest } from './types';

export const DICTIONARY_SCOPE_CLASS = 'yomitan-content';

export function StructuredContent({
  glossary,
  onLookup,
  className,
}: {
  glossary: Glossary;
  onLookup?: (request: LookupRequest) => void;
  className?: string;
}) {
  const content = typeof glossary === 'object' && glossary !== null && !Array.isArray(glossary)
    ? (glossary as { content?: unknown }).content
    : glossary;
  const rendered = useMemo(() => renderStructuredContent(content, onLookup), [content, onLookup]);
  return <div className={[DICTIONARY_SCOPE_CLASS, className].filter(Boolean).join(' ')}>{rendered}</div>;
}

export type { ReactNode };
