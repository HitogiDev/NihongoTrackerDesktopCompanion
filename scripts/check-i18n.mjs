// Every locale must have exactly the keys English has, and each string the
// same {{placeholders}}: a missing key silently falls back to English, and a
// renamed placeholder renders as literal braces. Run by `npm run check:i18n`.
import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../src/i18n/locales');
const BASE = 'en';

function flatten(node, prefix = '', out = new Map()) {
  for (const [key, value] of Object.entries(node)) {
    const dotted = prefix ? `${prefix}.${key}` : key;
    if (value && typeof value === 'object') flatten(value, dotted, out);
    else out.set(dotted, String(value));
  }
  return out;
}

function placeholders(text) {
  return [...text.matchAll(/\{\{\s*([\w.]+)\s*(?:,[^}]*)?\}\}/g)].map((m) => m[1]).sort().join(',');
}

function load(locale) {
  const dir = path.join(root, locale);
  const files = readdirSync(dir).filter((file) => file.endsWith('.json'));
  return new Map(files.map((file) => [file, flatten(JSON.parse(readFileSync(path.join(dir, file), 'utf8')))]));
}

const problems = [];
const base = load(BASE);
const locales = readdirSync(root, { withFileTypes: true })
  .filter((entry) => entry.isDirectory() && entry.name !== BASE)
  .map((entry) => entry.name);

for (const locale of locales) {
  const other = load(locale);
  for (const file of new Set([...base.keys(), ...other.keys()])) {
    const a = base.get(file);
    const b = other.get(file);
    if (!a || !b) {
      problems.push(`${locale}/${file}: only in ${a ? BASE : locale}`);
      continue;
    }
    for (const key of a.keys()) {
      if (!b.has(key)) problems.push(`${locale}/${file}: missing ${key}`);
      else if (placeholders(a.get(key)) !== placeholders(b.get(key))) {
        problems.push(`${locale}/${file}: ${key} uses {{${placeholders(b.get(key))}}}, ${BASE} uses {{${placeholders(a.get(key))}}}`);
      }
    }
    for (const key of b.keys()) {
      if (!a.has(key)) problems.push(`${locale}/${file}: extra ${key} (not in ${BASE})`);
    }
  }
}

if (problems.length) {
  console.error(`i18n: ${problems.length} problem(s)\n  ${problems.join('\n  ')}`);
  process.exit(1);
}
console.log(`i18n: ${locales.join(', ')} match ${BASE} (${[...base.values()].reduce((n, keys) => n + keys.size, 0)} keys)`);
