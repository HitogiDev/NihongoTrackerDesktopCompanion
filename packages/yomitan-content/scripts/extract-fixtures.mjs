// Pick real Jitendex entries that between them exercise every tag, hook and
// class the renderer claims to support, and write them out as a test fixture.
//
//   node scripts/extract-fixtures.mjs <dir with term_bank_*.json> <out.json>
//
// Greedy set cover: an entry is kept only if it brings something no kept entry
// has yet, so 30 entries reach full coverage instead of 30 near-duplicates.
import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const [, , dir, out] = process.argv;
if (!dir || !out) {
  console.error('usage: node extract-fixtures.mjs <term-bank-dir> <out.json>');
  process.exit(1);
}

const WANTED_TAGS = ['div', 'span', 'li', 'ul', 'ol', 'ruby', 'rt', 'a'];
const WANTED_HOOKS = [
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
  'example-sentence',
  'example-sentence-a',
  'example-sentence-b',
  'example-keyword',
];
const WANTED_CLASSES = ['tag', 'extra-box', 'form-valid'];
// Named separately so the fixture is guaranteed to exercise them even if the
// greedy pass would otherwise be satisfied early.
const WANTED_SHAPES = ['nested-ruby', 'internal-link', 'external-link', 'list-style-type', 'unsupported-tag'];

const MAX_ENTRIES = 30;

/**
 * `inline` tracks whether we are already inside a span or an `a`. A ruby found
 * there is the case that breaks naive renderers — furigana inside a link or
 * inside a highlighted keyword — so it gets its own marker.
 */
function features(node, found, inline = false) {
  if (node === null || node === undefined) return;
  if (typeof node !== 'object') return;
  if (Array.isArray(node)) {
    for (const child of node) features(child, found, inline);
    return;
  }

  if (typeof node.tag === 'string') {
    found.add(`tag:${node.tag}`);
    if (['table', 'tr', 'th', 'td', 'img'].includes(node.tag)) found.add('shape:unsupported-tag');
    if (node.tag === 'ruby' && inline) found.add('shape:nested-ruby');
  }
  if (node.data?.content) found.add(`hook:${node.data.content}`);
  if (node.data?.class) found.add(`class:${node.data.class}`);
  if (node.style?.listStyleType !== undefined) found.add('shape:list-style-type');
  if (typeof node.href === 'string') {
    found.add(node.href.startsWith('?') ? 'shape:internal-link' : 'shape:external-link');
  }

  features(node.content, found, inline || node.tag === 'span' || node.tag === 'a');
}

const target = new Set([
  ...WANTED_TAGS.map((t) => `tag:${t}`),
  ...WANTED_HOOKS.map((h) => `hook:${h}`),
  ...WANTED_CLASSES.map((c) => `class:${c}`),
  ...WANTED_SHAPES.map((s) => `shape:${s}`),
]);

const covered = new Set();
const picked = [];
/** Feature signatures already represented, so the filler stays varied. */
const signatures = new Set();

const files = readdirSync(dir)
  .filter((f) => /^term_bank_\d+\.json$/.test(f))
  .sort((a, b) => Number(a.match(/\d+/)[0]) - Number(b.match(/\d+/)[0]));

function keep(entry, found, reason) {
  for (const f of found) if (target.has(f)) covered.add(f);
  signatures.add([...found].sort().join('|'));
  picked.push({
    expression: entry[0],
    reading: entry[1],
    definitionTags: entry[2],
    rules: entry[3],
    glossary: entry[5],
    covers: [...found].filter((f) => target.has(f)).sort(),
    keptFor: reason,
  });
}

// Two passes over the same banks: first take whatever adds coverage, then fill
// up to MAX_ENTRIES with structurally different entries. One pass alone gives
// either six entries or thirty near-duplicates.
for (const pass of ['coverage', 'variety']) {
  for (const file of files) {
    if (picked.length >= MAX_ENTRIES) break;
    const bank = JSON.parse(readFileSync(join(dir, file), 'utf8'));
    for (const entry of bank) {
      if (picked.length >= MAX_ENTRIES) break;
      const found = new Set();
      for (const gloss of entry[5] ?? []) {
        if (typeof gloss === 'string') found.add('shape:plain-string');
        else features(gloss.content, found);
      }

      if (pass === 'coverage') {
        if ([...found].some((f) => target.has(f) && !covered.has(f))) keep(entry, found, 'coverage');
      } else {
        const signature = [...found].sort().join('|');
        if (!signatures.has(signature) && found.size >= 6) keep(entry, found, 'variety');
      }
    }
  }
}

const missing = [...target].filter((f) => !covered.has(f)).sort();
writeFileSync(out, `${JSON.stringify(picked, null, 2)}\n`, 'utf8');

console.log(`entries: ${picked.length}`);
console.log(`covered: ${covered.size}/${target.size}`);
if (missing.length) console.log(`MISSING: ${missing.join(', ')}`);
