#!/usr/bin/env bash
# Does a GCC 14 build with the C++ runtime linked statically run on Ubuntu 24.04,
# whose own GCC is 13 and whose libstdc++ is therefore too old?
#
# If it does, the Linux baseline stops being a decision and becomes CI
# configuration: build in a newer container, ship to the LTS.
#
# Run from the repo root:
#   docker run --rm -v "$PWD":/work gcc:14 bash /work/crates/nihongo-dict/build-linux-portable.sh
set -euo pipefail

apt-get update -qq >/dev/null
apt-get install -y -qq cmake ninja-build >/dev/null
echo "builder: $(g++ --version | head -1)"

rm -rf /b && mkdir -p /b
tar xzf /work/spike-data/hoshidicts-src.tgz -C /b

# -static-libstdc++ -static-libgcc: the two runtimes whose newer symbols would
# otherwise be missing on the LTS.
cmake -S /b/hoshidicts -B /b/out -G Ninja -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_EXE_LINKER_FLAGS="-static-libstdc++ -static-libgcc" >/dev/null
cmake --build /b/out --target hoshidicts-cli >/dev/null

echo "built: $(ls -l /b/out/hoshidicts-cli | awk '{printf "%.2f MB", $5/1048576}')"
echo
echo "dynamic dependencies:"
ldd /b/out/hoshidicts-cli || true
echo
echo "highest glibc symbol version required:"
objdump -T /b/out/hoshidicts-cli 2>/dev/null | grep -o 'GLIBC_[0-9.]*' | sort -V -u | tail -3
echo "libstdc++ referenced:"
objdump -T /b/out/hoshidicts-cli 2>/dev/null | grep -c GLIBCXX || echo 0

mkdir -p /work/spike-data/portable
cp /b/out/hoshidicts-cli /work/spike-data/portable/
echo PORTABLE-BUILD-OK
