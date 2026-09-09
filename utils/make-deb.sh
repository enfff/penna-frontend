#!/usr/bin/env bash
# Build a Debian (.deb) package of Diary.
#
# No dpkg required: a .deb is an ar archive with three members
# (debian-binary, control.tar.gz, data.tar.gz), assembled here with binutils'
# ar and tar. File ownership is forced to root:root via tar flags, so
# fakeroot is not needed either.
#
# Usage: utils/make-deb.sh
# Output: utils/out/penna-frontend_<version>_amd64.deb
#
# The postinst runs glib-compile-schemas, gtk-update-icon-cache and
# update-desktop-database on the target system.

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && git rev-parse --show-toplevel)"
OUT="$REPO/utils/out"

for dep in meson ninja ar tar; do
  command -v "$dep" >/dev/null || { echo "missing dependency: $dep" >&2; exit 1; }
done

BASE_VERSION="$(sed -nE "s/^[[:space:]]*version: '([^']+)'.*/\1/p" "$REPO/meson.build" | head -n1)"
[[ -n "$BASE_VERSION" ]] || { echo "cannot parse version from meson.build" >&2; exit 1; }
REV="$(git -C "$REPO" rev-list --count HEAD)"
SHA="$(git -C "$REPO" rev-parse --short=7 HEAD)"
VERSION="${BASE_VERSION}+git${REV}.g${SHA}"

STAGE="$(mktemp -d /tmp/penna-deb.XXXXXX)"
WORK="$(mktemp -d /tmp/penna-deb-data.XXXXXX)"
trap 'rm -rf "$STAGE" "$WORK"' EXIT

cd "$REPO"
meson setup "$WORK/build" --prefix=/usr --buildtype=release --wrap-mode=nofallback >/dev/null
meson install -C "$WORK/build" --destdir "$STAGE" >/dev/null

[[ -x "$STAGE/usr/bin/penna-frontend" ]] || { echo "build produced no binary" >&2; exit 1; }

mkdir -p "$STAGE/DEBIAN"
INSTALLED_KB="$(du -sk "$STAGE/usr" | cut -f1)"
cat > "$STAGE/DEBIAN/control" << EOF
Package: penna-frontend
Version: $VERSION
Architecture: amd64
Maintainer: Francesco P. Carmone <enforcetitan@gmail.com>
Depends: libgtk-4-1, libadwaita-1-0, libglib2.0-0, libpango-1.0-0, libcairo2, libdbus-1-3
Section: gnome
Priority: optional
Homepage: https://github.com/enfff/penna-frontend
Installed-Size: $INSTALLED_KB
Description: Local-first journaling app for GNOME
 Diary is a journal app built on the penna engine. Entries are plain
 Markdown files inside a git repository, so history and sync come from
 git and notes stay readable without the app.
EOF

cat > "$STAGE/DEBIAN/postinst" << 'EOF'
#!/bin/sh
set -e
glib-compile-schemas /usr/share/glib-2.0/schemas 2>/dev/null || true
gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor 2>/dev/null || true
update-desktop-database /usr/share/applications 2>/dev/null || true
EOF
chmod 755 "$STAGE/DEBIAN/postinst"

# control.tar.gz: DEBIAN contents without the DEBIAN prefix.
tar -czf "$WORK/control.tar.gz" -C "$STAGE/DEBIAN" .
# data.tar.gz: everything else, forced to root:root.
tar --owner=0 --group=0 --numeric-owner -czf "$WORK/data.tar.gz" -C "$STAGE" usr

printf '2.0\n' > "$WORK/debian-binary"
DEB="$OUT/penna-frontend_${VERSION}_amd64.deb"
mkdir -p "$OUT"
rm -f "$DEB"
ar -r "$DEB" "$WORK/debian-binary" "$WORK/control.tar.gz" "$WORK/data.tar.gz" >/dev/null

echo "deb ready: $DEB"
