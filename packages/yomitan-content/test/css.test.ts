import { readFileSync } from 'node:fs';

import { describe, expect, it } from 'vitest';

import { scopeDictionaryCss } from '../src/css';
import { DICTIONARY_SCOPE_CLASS } from '../src/StructuredContent';

describe('scopeDictionaryCss', () => {
  it('prefixes every top-level selector', () => {
    const { css } = scopeDictionaryCss(
      'span[title] { cursor: help; }\nul, ol { margin: 0; }',
      DICTIONARY_SCOPE_CLASS
    );
    expect(css).toContain('.yomitan-content span[title]');
    expect(css).toContain('.yomitan-content ul, .yomitan-content ol');
  });

  it('leaves nested rules alone, because they already inherit the scope', () => {
    const { css } = scopeDictionaryCss(
      'li[data-sc-content="sense"] { padding-left: 0.25em; & ul { list-style-type: none; } }',
      DICTIONARY_SCOPE_CLASS
    );
    expect(css).toContain('.yomitan-content li[data-sc-content="sense"]');
    // One scope, not two.
    expect(css.match(/\.yomitan-content/g)).toHaveLength(1);
    expect(css).toContain('& ul');
  });

  it('pins :root, html and body to the scope so they cannot escape it', () => {
    const { css } = scopeDictionaryCss(
      ':root { --x: 1; }\nbody { background: red; }\nhtml.dark { color: blue; }',
      DICTIONARY_SCOPE_CLASS
    );
    expect(css).not.toMatch(/(^|\s):root\s*\{/);
    expect(css).not.toMatch(/(^|\n)body\s*\{/);
    expect(css.match(/\.yomitan-content \{/g)).toHaveLength(3);
  });

  it('drops @import and other statement at-rules', () => {
    const result = scopeDictionaryCss(
      '@charset "utf-8";\n@import url("https://evil.test/x.css");\nspan { color: red; }',
      DICTIONARY_SCOPE_CLASS
    );
    expect(result.css).not.toContain('@import');
    expect(result.css).not.toContain('@charset');
    expect(result.css).toContain('.yomitan-content span');
    expect(result.droppedAtRules).toEqual({ '@charset': 1, '@import': 1 });
  });

  it('drops @font-face, which would fetch from wherever it likes', () => {
    const result = scopeDictionaryCss(
      '@font-face { font-family: x; src: url(https://evil.test/f.woff2); }',
      DICTIONARY_SCOPE_CLASS
    );
    expect(result.css).not.toContain('@font-face');
    expect(result.droppedAtRules['@font-face']).toBe(1);
  });

  it('keeps @media and scopes what is inside it', () => {
    const { css } = scopeDictionaryCss(
      '@media (min-width: 40rem) { span { color: red; } }',
      DICTIONARY_SCOPE_CLASS
    );
    expect(css).toContain('@media (min-width: 40rem)');
    expect(css).toContain('.yomitan-content span');
  });

  it('drops a declaration that fetches from outside, and keeps data: URIs', () => {
    const result = scopeDictionaryCss(
      'span { background: url(https://evil.test/pixel.png); color: red; }\n' +
        'div { background: url("data:image/gif;base64,R0lGOD"); }',
      DICTIONARY_SCOPE_CLASS
    );
    expect(result.css).not.toContain('evil.test');
    expect(result.css).toContain('color: red');
    expect(result.css).toContain('data:image/gif');
    expect(result.droppedDeclarations).toBe(1);
  });

  it('strips comments, including one hiding a brace', () => {
    const { css } = scopeDictionaryCss('/* } tricky */ span { color: red; }', DICTIONARY_SCOPE_CLASS);
    expect(css).not.toContain('tricky');
    expect(css).toContain('.yomitan-content span');
  });

  it('survives unbalanced input without throwing', () => {
    expect(() => scopeDictionaryCss('span { color: red;', DICTIONARY_SCOPE_CLASS)).not.toThrow();
    expect(() => scopeDictionaryCss('}}}{{{', DICTIONARY_SCOPE_CLASS)).not.toThrow();
    expect(scopeDictionaryCss('', DICTIONARY_SCOPE_CLASS).css).toBe('');
  });
});

describe("Jitendex's own stylesheet", () => {
  const path = new URL('./fixtures/jitendex-styles.css', import.meta.url);
  const css = readFileSync(path, 'utf8');

  it('scopes every rule in it', () => {
    const result = scopeDictionaryCss(css, DICTIONARY_SCOPE_CLASS);
    expect(result.droppedAtRules).toEqual({});
    expect(result.droppedDeclarations).toBe(0);

    // Every top-level rule is under the scope. Nested rules (indented, or
    // starting with `&`) are deliberately left alone: they already inherit it.
    for (const line of result.css.split('\n')) {
      if (!line.trimEnd().endsWith('{')) continue;
      if (line.startsWith('@') || line.startsWith(' ') || line.startsWith('\t')) continue;
      expect(line, line).toContain('.yomitan-content');
    }
  });

  it('keeps the attribute selectors the renderer emits', () => {
    const { css: scoped } = scopeDictionaryCss(css, DICTIONARY_SCOPE_CLASS);
    expect(scoped).toContain('[data-sc-content="sense-groups"]');
    expect(scoped).toContain('[data-sc-class="tag"]');
  });

  it('matches its snapshot', () => {
    expect(scopeDictionaryCss(css, DICTIONARY_SCOPE_CLASS).css).toMatchSnapshot();
  });
});
