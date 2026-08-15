import { isValidElement, type ReactElement, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';

import { StructuredContent } from '../src/StructuredContent';
import { createDiagnostics, type DiagnosticsCollector } from '../src/diagnostics';
import type { Glossary, LookupRequest } from '../src/types';

export interface FixtureEntry {
  expression: string;
  reading: string;
  definitionTags: string;
  rules: string;
  glossary: Glossary[];
  covers: string[];
  keptFor: string;
}

export function renderGlossary(
  glossary: Glossary,
  onLookup?: (request: LookupRequest) => void
): { html: string; diagnostics: DiagnosticsCollector } {
  const diagnostics = createDiagnostics();
  const html = renderToStaticMarkup(
    <StructuredContent glossary={glossary} onLookup={onLookup} diagnostics={diagnostics} />
  );
  return { html, diagnostics };
}

/**
 * Walk a rendered React tree looking for elements whose props match, so a
 * handler can be exercised without standing up a DOM.
 */
export function findElements(
  node: ReactNode,
  predicate: (element: ReactElement) => boolean
): ReactElement[] {
  const found: ReactElement[] = [];

  const visit = (current: ReactNode): void => {
    if (Array.isArray(current)) {
      for (const child of current) visit(child);
      return;
    }
    if (!isValidElement(current)) return;
    if (predicate(current)) found.push(current);
    const props = current.props as { children?: ReactNode };
    if (props?.children !== undefined) visit(props.children);
  };

  visit(node);
  return found;
}

/** Readable snapshots: one tag per line, so a diff points at the node. */
export function prettifyHtml(html: string): string {
  return html.replace(/></g, '>\n<');
}
