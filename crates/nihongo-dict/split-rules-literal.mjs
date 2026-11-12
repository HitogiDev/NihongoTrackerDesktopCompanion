// MSVC caps a single string literal at 16,380 bytes (C2026). hoshidicts embeds
// its deconjugation rules as one 35 KB raw literal, so the header has to be
// rewritten as adjacent chunks, which the compiler concatenates (the limit on
// the *concatenated* result is 65,535 bytes, and 35 KB fits).
//
// Splitting happens at line boundaries so the JSON inside stays readable.
import { readFileSync, writeFileSync } from 'node:fs';

const path = process.argv[2];
const CHUNK_BYTES = 8000;

// Normalised to LF: the file is stored with LF upstream, Windows checks it out
// as CRLF, and mixing the two while cutting the literal would leave stray CRs
// inside the JSON.
const source = readFileSync(path, 'utf8').replace(/\r\n/g, '\n');
const open = 'R"json(';
const close = ')json"';
const start = source.indexOf(open);
const end = source.lastIndexOf(close);
if (start < 0 || end < 0) {
  console.error('the raw literal is not where it was expected');
  process.exit(1);
}

const body = source.slice(start + open.length, end);
if (body.includes(close)) {
  console.error('the body already contains a delimiter; refusing to split');
  process.exit(1);
}

const lines = body.split('\n');
const chunks = [];
let current = '';
for (const line of lines) {
  const piece = current === '' ? line : `${current}\n${line}`;
  if (Buffer.byteLength(piece, 'utf8') > CHUNK_BYTES && current !== '') {
    chunks.push(current);
    current = line;
  } else {
    current = piece;
  }
}
if (current !== '') chunks.push(current);

const joined = chunks.map((chunk) => `${open}${chunk}${close}`).join('\n');
writeFileSync(path, source.slice(0, start) + joined + source.slice(end + close.length), 'utf8');

const sizes = chunks.map((c) => Buffer.byteLength(c, 'utf8'));
console.log(`chunks: ${chunks.length}, bytes: ${sizes.join(', ')}, total: ${sizes.reduce((a, b) => a + b, 0)}`);
