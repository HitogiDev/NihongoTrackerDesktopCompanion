import { describe, expect, it } from 'vitest';

import { classifyHref } from '../src/href';

describe('classifyHref', () => {
  it('accepts the internal cross-reference form Yomitan writes', () => {
    const policy = classifyHref(
      '?query=%E5%90%8C%E4%B8%8A&wildcards=off&primary_reading=%E3%81%A9%E3%81%86%E3%81%98%E3%82%87%E3%81%86'
    );
    expect(policy).toEqual({
      kind: 'lookup',
      request: { query: '同上', reading: 'どうじょう' },
    });
  });

  it('accepts a query with no reading', () => {
    expect(classifyHref('?query=%E7%8C%AB&wildcards=off')).toEqual({
      kind: 'lookup',
      request: { query: '猫' },
    });
  });

  it.each([
    ['https://tatoeba.org/en/sentences/show/193344', 'external'],
    ['http://example.test', 'external'],
    ['//example.test/x', 'external'],
    ['javascript:alert(1)', 'external'],
    ['JavaScript:alert(1)', 'external'],
    ['data:text/html,<script>', 'external'],
    ['vbscript:msgbox', 'external'],
    ['/absolute/path', 'external'],
    ['relative.html', 'external'],
    ['', 'malformed'],
    ['?query=x#fragment', 'malformed'],
    ['?query=x\\y', 'malformed'],
    ['?wildcards=off', 'missing-query'],
    ['?query=', 'missing-query'],
    ['?query=%20%20', 'missing-query'],
  ])('renders %s as text (%s)', (href, reason) => {
    expect(classifyHref(href)).toEqual({ kind: 'text', reason });
  });

  it('refuses an absurdly long query rather than passing it on', () => {
    expect(classifyHref(`?query=${'あ'.repeat(200)}`)).toEqual({
      kind: 'text',
      reason: 'oversized',
    });
  });

  it('refuses anything that is not a string', () => {
    for (const value of [undefined, null, 42, {}, []]) {
      expect(classifyHref(value)).toEqual({ kind: 'text', reason: 'malformed' });
    }
  });

  it('drops an oversized reading but keeps the query', () => {
    const policy = classifyHref(`?query=%E7%8C%AB&primary_reading=${'ね'.repeat(200)}`);
    expect(policy).toEqual({ kind: 'lookup', request: { query: '猫' } });
  });
});
