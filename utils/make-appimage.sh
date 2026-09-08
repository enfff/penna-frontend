#!/usr/bin/env bash
# Build a self-contained AppImage of Diary.
#
# Recipe: meson-install the current checkout into an AppDir, then let
# linuxdeploy + linuxdeploy-plugin-gtk bundle the GTK4/libadwaita runtime
# (shared libraries, glib schemas, gio modules, pixbuf loaders, icon caches).
#
# Usage: utils/make-appimage.sh
# Output: utils/out/Diary-<version>-<arch>.AppImage
#
# Tool images are cached in ~/.cache/penna-appimage. FUSE is not required:
# linuxdeploy runs with APPIMAGE_EXTRACT_AND_RUN=1.

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && git rev-parse --show-toplevel)"
OUT="$REPO/utils/out"
CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/penna-appimage"
ARCH="${ARCH:-x86_64}"
VERSION="$(git -C "$REPO" describe --tags --always --abbrev=7 | sed 's/^v//')"
DESKTOP="io.github.enfff.Diary"

LD="$CACHE/linuxdeploy-$ARCH.AppImage"
# The gtk plugin ships as a plain script; linuxdeploy discovers plugins named
# linuxdeploy-plugin-* next to its own binary.
GTK_PLUGIN="$CACHE/linuxdeploy-plugin-gtk"

for dep in meson ninja curl; do
  command -v "$dep" >/dev/null || { echo "missing dependency: $dep" >&2; exit 1; }
done

mkdir -p "$CACHE" "$OUT"
fetch() {
  local dest="$1" url="$2"
  if [[ ! -x "$dest" ]]; then
    echo "fetching $(basename "$dest")"
    curl -L --fail --retry 3 -o "$dest" "$url"
  fi
  chmod +x "$dest"
}
fetch "$LD" "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-$ARCH.AppImage"
fetch "$GTK_PLUGIN" "https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gtk/master/linuxdeploy-plugin-gtk.sh"

BUILD="$(mktemp -d /tmp/penna-appimage.XXXXXX)"
APPDIR="$BUILD/AppDir"
trap 'rm -rf "$BUILD"' EXIT

cd "$REPO"
meson setup "$BUILD/build" --prefix=/usr --buildtype=release --wrap-mode=nofallback >/dev/null
meson install -C "$BUILD/build" --destdir "$APPDIR" >/dev/null

[[ -x "$APPDIR/usr/bin/penna-frontend" ]] || { echo "build produced no binary" >&2; exit 1; }

# linuxdeploy-plugin-gtk bundles the libraries but not the Adwaita icon
# theme, and it copies host gschema overrides without compiling them. Both
# are required for libadwaita to look like libadwaita.
if [[ -d /usr/share/icons/Adwaita ]]; then
  cp -r /usr/share/icons/Adwaita "$APPDIR/usr/share/icons/Adwaita"
fi
command -v glib-compile-schemas >/dev/null && \
  glib-compile-schemas "$APPDIR/usr/share/glib-2.0/schemas" >/dev/null
if command -v gtk-update-icon-cache >/dev/null; then
  for theme in hicolor Adwaita; do
    gtk-update-icon-cache -q -t -f "$APPDIR/usr/share/icons/$theme" 2>/dev/null || true
  done
fi

export APPIMAGE_EXTRACT_AND_RUN=1
export OUTPUT="$BUILD/Diary-$VERSION-$ARCH.AppImage"
env APPDIR="$APPDIR" "$LD" \
  --appdir "$APPDIR" \
  -e "$APPDIR/usr/bin/penna-frontend" \
  -d "$APPDIR/usr/share/applications/$DESKTOP.desktop" \
  -i "$APPDIR/usr/share/icons/hicolor/scalable/apps/$DESKTOP.svg" \
  --plugin gtk \
  --output appimage

mv "$OUTPUT" "$OUT/"
echo "AppImage ready: $OUT/$(basename "$OUTPUT")"
