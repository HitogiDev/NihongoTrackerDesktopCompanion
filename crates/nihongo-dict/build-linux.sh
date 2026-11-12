#!/usr/bin/env bash
# Spike helper: build hoshidicts on Linux inside a container, so the result says
# something about a clean checkout rather than about this machine.
#
# GCC 14+ is required: cli/main.cpp uses std::ranges::to, which GCC 13 (the
# default on Ubuntu 24.04) does not have.
#
# The sources arrive as a tarball. Compiling straight off the Windows-hosted
# bind mount is dominated by filesystem latency — copying the tree file by file
# took longer than the build itself.
set -euo pipefail

apt-get update -qq >/dev/null
apt-get install -y -qq cmake ninja-build >/dev/null
echo "toolchain: $(g++ --version | head -1) | $(cmake --version | head -1)"

rm -rf /build && mkdir -p /build
tar xzf /work/spike-data/hoshidicts-src.tgz -C /build

start=$(date +%s)
cmake -S /build/hoshidicts -B /build/out -G Ninja -DCMAKE_BUILD_TYPE=Release >/dev/null
cmake --build /build/out --target hoshidicts hoshidicts-cli >/dev/null
echo "clean build: $(( $(date +%s) - start ))s"

ls -l /build/out/libhoshidicts.a /build/out/hoshidicts-cli | awk '{printf "%s  %.2f MB\n", $9, $5/1048576}'

# Keep the CLI so the same binary can be measured against the same dictionary.
mkdir -p /work/spike-data/linux
cp /build/out/hoshidicts-cli /work/spike-data/linux/
echo BUILD-OK
