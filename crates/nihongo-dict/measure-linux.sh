#!/usr/bin/env bash
# Spike measurements on Linux: import Jitendex, then look a heavily conjugated
# verb up. Runs off the container filesystem, not the Windows bind mount, so the
# numbers are about the library rather than about 9p.
#
# No `head`/`tail` in pipelines here: with `set -o pipefail` the SIGPIPE from a
# closing `head` aborts the whole script.
set -euo pipefail

WORD=${1:-食べさせられなかった}
CLI=/build/out/hoshidicts-cli
OUT=/work/spike-data/linux
mkdir -p "$OUT" /build/dicts
cp /work/spike-data/jitendex-yomitan.zip /build/dicts/

echo "=== import (cold, from the container filesystem)"
/usr/bin/time -f "wall: %es  peak RSS: %MkB" $CLI import /build/dicts/jitendex-yomitan.zip

DICT=$(find /build/dicts -maxdepth 1 -mindepth 1 -type d | head -1 || true)
echo
echo "=== on-disk"
du -sb /build/dicts/jitendex-yomitan.zip
du -sb "$DICT"
echo "files in the imported dictionary: $(find "$DICT" -type f | wc -l)"
ls -l "$DICT"

echo
echo "=== deconjugation"
$CLI deconjugate "$WORD" > "$OUT/deconjugate.txt"
echo "candidates: $(grep -c 'process:' "$OUT/deconjugate.txt" || true)"
sed -n '1,12p' "$OUT/deconjugate.txt"

echo
echo "=== lookup, one dictionary"
$CLI lookup "$DICT" "$WORD" > "$OUT/lookup.txt"
sed -n '1,45p' "$OUT/lookup.txt"

echo
echo "=== lookup, two term dictionaries (same data imported twice, to see"
echo "    whether glossaries follow the order the dictionaries were added)"
cp -r "$DICT" "/build/dicts/second-copy"
$CLI lookup "$DICT" "/build/dicts/second-copy" "$WORD" > "$OUT/lookup-two.txt"
grep -n "dict_name\|Jitendex\|second-copy\|^[0-9]* results" "$OUT/lookup-two.txt" | head -20 || true
echo "glossary blocks in the two-dictionary run: $(grep -c '^Jitendex' "$OUT/lookup-two.txt" || true)"

echo
echo "=== repeated lookups (each run re-opens the dictionary: upper bound)"
for i in 1 2 3 4 5; do
  $CLI lookup "$DICT" "$WORD" > /tmp/run.txt
  grep '^runtime:' /tmp/run.txt
done
echo "DONE"
