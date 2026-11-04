#!/usr/bin/env bash
# Removes the libwayland copies linuxdeploy bundles into the AppImage, in place.
#
# They come from the Ubuntu build machine and clash with a newer host Mesa
# (Arch, Fedora…): WebKitGTK aborts with "Could not create default EGL display:
# EGL_BAD_PARAMETER" and the window stays blank. Every desktop that runs GTK
# has its own libwayland, so the host's is the one to load.
#
# Repacking changes the bytes: the AppImage has to be signed again afterwards.
#
#   scripts/appimage-unbundle-wayland.sh path/to/App.AppImage
set -euo pipefail

appimage=$(realpath "$1")
appimagetool_version=1.9.1
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cd "$work"

chmod +x "$appimage"
# Neither needs FUSE, which CI runners do not have.
"$appimage" --appimage-extract >/dev/null
export APPIMAGE_EXTRACT_AND_RUN=1

removed=$(find squashfs-root -name 'libwayland-*.so*' -print -delete)
if [ -z "$removed" ]; then
  echo "No bundled libwayland in $appimage; nothing to do."
  exit 0
fi
echo "Removed:"
echo "$removed"

curl -fsSL -o appimagetool \
  "https://github.com/AppImage/appimagetool/releases/download/$appimagetool_version/appimagetool-x86_64.AppImage"
chmod +x appimagetool
ARCH=x86_64 ./appimagetool --no-appstream squashfs-root repacked.AppImage >/dev/null
mv repacked.AppImage "$appimage"
chmod +x "$appimage"
