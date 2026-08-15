import { describe, expect, it } from 'vitest';

import entries from './fixtures/jitendex-entries.json';
import { findElements, prettifyHtml, renderGlossary, type FixtureEntry } from './helpers';
import { renderStructuredContent } from '../src/render';
import { createDiagnostics } from '../src/diagnostics';
import type { LookupRequest } from '../src/types';

const fixtures = entries as unknown as FixtureEntry[];

describe('the fixture itself', () => {
  it('is 30 real Jitendex entries covering every supported tag, hook and class', () => {
    expect(fixtures).toHaveLength(30);

    const covered = [...new Set(fixtures.flatMap((entry) => entry.covers))];
    for (const tag of ['div', 'span', 'li', 'ul', 'ol', 'ruby', 'rt', 'a']) {
      expect(covered, `tag ${tag}`).toContain(`tag:${tag}`);
    }
    for (const hook of [
      'part-of-speech-info',
      'sense',
      'glossary',
      'sense-group',
      'attribution',
      'forms',
      'redirect-glossary',
      'extra-info',
      'misc-info',
      'reference-label',
      'xref',
    ]) {
      expect(covered, `hook ${hook}`).toContain(`hook:${hook}`);
    }
    for (const styleClass of ['tag', 'extra-box', 'form-valid']) {
      expect(covered, `class ${styleClass}`).toContain(`class:${styleClass}`);
    }
    // The two shapes that break naive renderers.
    expect(covered).toContain('shape:nested-ruby');
    expect(covered).toContain('shape:internal-link');
  });
});

describe('rendering real entries', () => {
  for (const [index, entry] of fixtures.entries()) {
    it(`renders ${entry.expression}【${entry.reading}】 (#${index + 1})`, () => {
      const rendered = entry.glossary.map((glossary) => {
        const { html } = renderGlossary(glossary, () => {});
        return prettifyHtml(html);
      });
      expect(rendered.join('\n\n')).toMatchSnapshot();
    });
  }
});

describe('what never reaches the DOM', () => {
  const everyHtml = fixtures.flatMap((entry) =>
    entry.glossary.map((glossary) => renderGlossary(glossary, () => {}).html)
  );

  it('never emits an href, on any entry', () => {
    for (const html of everyHtml) expect(html).not.toContain('href=');
  });

  it('never emits a tag outside the whitelist', () => {
    // `button` is ours, for cross-references; the rest are the eight supported
    // tags plus the wrapper div.
    const allowed = ['div', 'span', 'li', 'ul', 'ol', 'ruby', 'rt', 'button'];
    for (const html of everyHtml) {
      for (const [, tag] of html.matchAll(/<([a-zA-Z][a-zA-Z0-9]*)/g)) {
        expect(allowed, `unexpected <${tag}>`).toContain(tag);
      }
    }
  });

  it('never emits a style attribute other than list-style-type', () => {
    for (const html of everyHtml) {
      for (const [, style] of html.matchAll(/style="([^"]*)"/g)) {
        expect(style).toMatch(/^list-style-type:/);
      }
    }
  });

  it('keeps the data-sc hooks the dictionary stylesheet selects on', () => {
    const joined = everyHtml.join('');
    expect(joined).toContain('data-sc-content="glossary"');
    expect(joined).toContain('data-sc-class="tag"');
  });
});

describe('unsupported nodes', () => {
  it('renders the contents of a table instead of dropping it, and counts it', () => {
    const diagnostics = createDiagnostics();
    renderStructuredContent(
      {
        tag: 'table',
        content: [
          { tag: 'tr', content: [{ tag: 'th', content: '見出し' }, { tag: 'td', content: '本文' }] },
        ],
      },
      { diagnostics }
    );

    const snapshot = diagnostics.snapshot();
    expect(snapshot.unsupportedTags).toEqual({ table: 1, tr: 1, th: 1, td: 1 });
    expect(diagnostics.isClean()).toBe(false);
  });

  it('keeps the text of an unsupported node in the output', () => {
    const { html } = renderGlossary({
      type: 'structured-content',
      content: { tag: 'table', content: { tag: 'td', content: '残す' } },
    });
    expect(html).toContain('残す');
    expect(html).not.toContain('<table');
  });

  it('keeps a hook it has no styling for, and counts it', () => {
    const { html, diagnostics } = renderGlossary({
      type: 'structured-content',
      content: { tag: 'div', data: { content: 'dialect-info' }, content: '関西弁' },
    });
    // The attribute survives so the dictionary's own stylesheet still matches,
    // even though this package has no opinion about how it looks.
    expect(html).toContain('data-sc-content="dialect-info"');
    expect(html).toContain('関西弁');
    expect(diagnostics.snapshot().unstyledHooks['dialect-info']).toBe(1);
  });

  it('drops an attribute whose value could not be a selector, and counts it', () => {
    const { html, diagnostics } = renderGlossary({
      type: 'structured-content',
      content: { tag: 'span', data: { content: 'evil"] , * { color: red } [x="' }, content: 'x' },
    });
    expect(html).not.toContain('data-sc-content');
    expect(diagnostics.snapshot().droppedAttributes['data.content']).toBe(1);
  });

  it('skips a node that is not content at all', () => {
    const diagnostics = createDiagnostics();
    renderStructuredContent([true as never, (() => {}) as never], { diagnostics });
    expect(diagnostics.snapshot().skippedNodes).toBe(1);
  });
});

describe('cross-references', () => {
  const withXref = fixtures.find((entry) => entry.covers.includes('shape:internal-link'));

  it('is present in the fixture', () => {
    expect(withXref).toBeDefined();
  });

  it('renders an internal link as a button, not an anchor', () => {
    const { html } = renderGlossary(withXref!.glossary[0], () => {});
    // Attribute order is React's business, so the two halves are checked apart.
    expect(html).toContain('<button');
    expect(html).toContain('type="button"');
    expect(html).not.toContain('<a ');
  });

  it('asks for a new lookup when the button is activated', () => {
    const requests: LookupRequest[] = [];
    const tree = renderStructuredContent(
      (withXref!.glossary[0] as { content: unknown }).content as never,
      { onLookup: (request) => requests.push(request) }
    );

    const buttons = findElements(tree, (element) => element.type === 'button');
    expect(buttons.length).toBeGreaterThan(0);

    const onClick = (buttons[0].props as { onClick?: () => void }).onClick;
    expect(onClick).toBeTypeOf('function');
    onClick!();

    expect(requests).toHaveLength(1);
    expect(requests[0].query.length).toBeGreaterThan(0);
    // Decoded, not the percent-encoded form that was in the href.
    expect(requests[0].query).not.toContain('%');
  });

  it('renders the link as plain text when no handler is supplied', () => {
    const { html, diagnostics } = renderGlossary(withXref!.glossary[0]);
    expect(html).not.toContain('<button');
    expect(diagnostics.snapshot().blockedLinks['no-handler']).toBeGreaterThan(0);
  });

  it('keeps furigana inside the cross-reference', () => {
    const { html } = renderGlossary(withXref!.glossary[0], () => {});
    // A ruby nested inside the link is the case a flattening renderer loses.
    expect(html).toMatch(/<button[^>]*>[\s\S]*<ruby>[\s\S]*<rt>/);
  });
});

describe('external links', () => {
  it('renders an attribution link as text and counts it', () => {
    const { html, diagnostics } = renderGlossary(
      {
        type: 'structured-content',
        content: {
          tag: 'a',
          href: 'https://www.edrdg.org/jmwsgi/entr.py?svc=jmdict&q=1358280',
          content: 'JMdict',
        },
      },
      () => {}
    );
    expect(html).toContain('JMdict');
    expect(html).not.toContain('<button');
    expect(html).not.toContain('href');
    expect(diagnostics.snapshot().blockedLinks.external).toBe(1);
  });

  it('refuses a javascript: URL the same way, as text', () => {
    const { html, diagnostics } = renderGlossary(
      {
        type: 'structured-content',
        // eslint-disable-next-line no-script-url
        content: { tag: 'a', href: 'javascript:alert(1)', content: 'click' },
      },
      () => {}
    );
    expect(html).toBe('<div class="yomitan-content text-base-content"><span>click</span></div>');
    expect(diagnostics.snapshot().blockedLinks.external).toBe(1);
  });
});
