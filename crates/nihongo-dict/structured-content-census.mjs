// Census of Yomitan structured content in an imported dictionary's term banks.
//
// Turns "we would have to write a renderer" into a finite list: which node tags
// appear, which data-content / data-class hooks the stylesheet keys off, which
// inline style properties are used, and how many entries each one touches.
//
//   node structured-content-census.mjs <dir with term_bank_*.json> [sample.json]
//
// The optional second argument writes out real Japanese example sentences found
// in the data, which the latency benchmark uses so it measures real text rather
// than something synthetic.
import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const dir = process.argv[2];
const samplePath = process.argv[3];
if (!dir) {
  console.error('usage: node structured-content-census.mjs <term-bank-dir> [sample.json]');
  process.exit(1);
}

const tags = new Map();
const contentHooks = new Map();
const classHooks = new Map();
const styleProps = new Map();
const attributes = new Map();
const glossaryShapes = new Map();

// Counted per entry, not per node, so the percentages mean "share of entries a
// renderer would get wrong if it did not support this".
let entries = 0;
const entryTouched = {
  tags: new Map(),
  contentHooks: new Map(),
  classHooks: new Map(),
  styleProps: new Map(),
};

const sentences = [];

function bump(map, key, by = 1) {
  map.set(key, (map.get(key) ?? 0) + by);
}

function walk(node, seen) {
  if (node === null || node === undefined) return;
  if (typeof node === 'string') return;
  if (Array.isArray(node)) {
    for (const child of node) walk(child, seen);
    return;
  }
  if (typeof node !== 'object') return;

  if (typeof node.tag === 'string') {
    bump(tags, node.tag);
    seen.tags.add(node.tag);
  }
  for (const key of Object.keys(node)) {
    if (!['tag', 'content', 'data', 'style'].includes(key)) {
      bump(attributes, key);
    }
  }
  if (node.data && typeof node.data === 'object') {
    for (const [key, value] of Object.entries(node.data)) {
      if (key === 'content') {
        bump(contentHooks, String(value));
        seen.contentHooks.add(String(value));
      } else if (key === 'class') {
        bump(classHooks, String(value));
        seen.classHooks.add(String(value));
      } else {
        bump(attributes, `data.${key}`);
      }
    }
  }
  if (node.style && typeof node.style === 'object') {
    for (const key of Object.keys(node.style)) {
      bump(styleProps, key);
      seen.styleProps.add(key);
    }
  }
  walk(node.content, seen);
}

/**
 * The text a reader actually sees: everything concatenated, with `rt` dropped so
 * furigana readings do not get spliced into the middle of words. Recursive,
 * because example sentences nest ruby inside spans inside the keyword markup —
 * a shallow pass silently loses a word or two per sentence.
 */
function plainText(node) {
  if (typeof node === 'string') return node;
  if (Array.isArray(node)) return node.map(plainText).join('');
  if (!node || typeof node !== 'object') return '';
  if (node.tag === 'rt') return '';
  return plainText(node.content);
}

/** Japanese sentences, for the latency benchmark. */
function collectSentence(node) {
  if (!node || typeof node !== 'object') return;
  if (Array.isArray(node)) {
    for (const child of node) collectSentence(child);
    return;
  }
  if (node.data?.content === 'example-sentence-a') {
    const text = plainText(node.content);
    if (/[。！？]$/.test(text) && /[぀-ヿ一-鿿]/.test(text)) sentences.push(text);
    return;
  }
  collectSentence(node.content);
}

const files = readdirSync(dir).filter((f) => /^term_bank_\d+\.json$/.test(f));
if (files.length === 0) {
  console.error(`no term_bank_*.json in ${dir}`);
  process.exit(1);
}

for (const file of files) {
  const bank = JSON.parse(readFileSync(join(dir, file), 'utf8'));
  for (const entry of bank) {
    entries += 1;
    const seen = {
      tags: new Set(),
      contentHooks: new Set(),
      classHooks: new Set(),
      styleProps: new Set(),
    };
    const glossary = entry[5] ?? [];
    for (const gloss of glossary) {
      if (typeof gloss === 'string') {
        bump(glossaryShapes, 'plain string');
        continue;
      }
      bump(glossaryShapes, gloss?.type ?? 'object without type');
      walk(gloss.content, seen);
      if (sentences.length < 40000) collectSentence(gloss.content);
    }
    for (const [bucket, set] of Object.entries(seen)) {
      for (const key of set) bump(entryTouched[bucket], key);
    }
  }
}

function table(title, counts, touched) {
  const rows = [...counts.entries()].sort((a, b) => b[1] - a[1]);
  console.log(`\n## ${title} (${rows.length} distinct)\n`);
  console.log('| item | nodes | entries | % of entries |');
  console.log('|---|---:|---:|---:|');
  for (const [key, count] of rows) {
    const hit = touched?.get(key) ?? 0;
    const pct = touched ? ((hit / entries) * 100).toFixed(2) : '';
    console.log(`| \`${key}\` | ${count.toLocaleString()} | ${hit.toLocaleString()} | ${pct} |`);
  }
}

console.log(`entries scanned: ${entries.toLocaleString()} across ${files.length} term banks`);
table('Glossary shapes', glossaryShapes);
table('Node tags', tags, entryTouched.tags);
table('data-sc-content hooks', contentHooks, entryTouched.contentHooks);
table('data-sc-class hooks', classHooks, entryTouched.classHooks);
table('Inline style properties', styleProps, entryTouched.styleProps);
table('Other attributes', attributes);

if (samplePath) {
  // Around 50 characters is a typical visual-novel line.
  const unique = [...new Set(sentences)].filter((s) => s.length >= 35 && s.length <= 70);
  const sample = unique.slice(0, 500);
  writeFileSync(samplePath, JSON.stringify(sample, null, 2), 'utf8');
  console.log(`\nwrote ${sample.length} sample sentences to ${samplePath}`);
}
