#!/usr/bin/env bash
# Fork hoshidicts and its six nested submodules into our own org, then repoint
# every submodule at the fork, pinned to the exact commit this spike measured.
#
# This is not run automatically: it needs a GitHub login this environment does
# not have, and it needs the org name.
#
#   ORG=NihongoTracker ./fork-and-repoint.sh
#
# Requires `gh auth login` first.
set -euo pipefail

ORG=${ORG:?set ORG to the GitHub organisation that should own the forks}
ROOT=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
VENDOR="$ROOT/crates/nihongo-dict/vendor"

# upstream repo                              pinned commit                             submodule path
FORKS=(
  "Manhhao/hoshidicts|af99b554cd4ab289aa65e16fd2a4eea0d3870d3b|"
  "stephenberry/glaze|be5159d80c480ec0d97db40f685983e2f7ade2d3|external/glaze"
  "ebiggers/libdeflate|4b6db597a58a92cf7f1e171211d718ac1faea845|external/libdeflate"
  "martinus/unordered_dense|7b55cab8418da1603496462ce3ccdb4cb1dc3368|external/unordered_dense"
  "nemtrif/utfcpp|cfc9112cee3e817e8b72948a675f78479546f0cf|external/utfcpp"
  "Cyan4973/xxHash|82cead715cbfddd9e6093db8df95155077ce6e64|external/xxHash"
  "facebook/zstd|0532fe3e8ac1caf48f11603847ac3176064189d1|external/zstd"
)

echo "== forking into $ORG"
for row in "${FORKS[@]}"; do
  IFS='|' read -r repo commit path <<<"$row"
  name=${repo#*/}
  if gh repo view "$ORG/$name" >/dev/null 2>&1; then
    echo "  $ORG/$name already exists"
  else
    gh repo fork "$repo" --org "$ORG" --clone=false
  fi
  # A fork only mirrors branches that existed at fork time; make sure the exact
  # commit we depend on is reachable in ours, not just in upstream.
  echo "  verifying $commit is present in $ORG/$name"
  gh api "repos/$ORG/$name/commits/$commit" --jq .sha >/dev/null
done

echo
echo "== repointing submodules"
# The outer submodule first, then the nested ones inside it.
git -C "$ROOT" submodule set-url crates/nihongo-dict/vendor "https://github.com/$ORG/hoshidicts.git"

for row in "${FORKS[@]:1}"; do
  IFS='|' read -r repo commit path <<<"$row"
  name=${repo#*/}
  git -C "$VENDOR" submodule set-url "$path" "https://github.com/$ORG/$name.git"
done

echo
echo "== re-syncing"
git -C "$ROOT" submodule sync --recursive
git -C "$ROOT" submodule update --init --recursive --depth 1

echo
echo "== pinned commits"
echo "hoshidicts $(git -C "$VENDOR" rev-parse HEAD)"
git -C "$VENDOR" submodule status

cat <<'NOTE'

Note: hoshidicts' own .gitmodules still lists the upstream URLs, so the URL
override above lives in our repo's .git/config rather than in a committed file.
To make it survive a fresh clone, commit the rewritten .gitmodules inside the
fork of hoshidicts itself and pin our submodule at that commit.
NOTE
