#!/usr/bin/env bash
# Build a local Arch Linux package (.pkg.tar.zst) from the current git HEAD.
#
# Reuses packaging/penna-frontend-git/PKGBUILD, with its source URL rewritten
# to clone from this repository instead of GitHub, so the generated package
# always matches the local checkout (github's master may lag behind).
#
# Usage: utils/make-arch-package.sh [makepkg args...]
#        default args: -f
#        e.g. utils/make-arch-package.sh -f -C   # also clean the src dir
#             utils/make-arch-package.sh -f -e   # skip meson test (check())
# Output: utils/out/<pkgname>-<pkgver>-<arch>.pkg.tar.zst

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && git rev-parse --show-toplevel)"
PKGDIR="$REPO/packaging/penna-frontend-git"
OUT="$REPO/utils/out"

command -v makepkg >/dev/null || { echo "makepkg not found (install pacman)" >&2; exit 1; }
[[ -f "$PKGDIR/PKGBUILD" ]] || { echo "missing $PKGDIR/PKGBUILD" >&2; exit 1; }

BUILD="$(mktemp -d /tmp/penna-arch-pkg.XXXXXX)"
trap 'rm -rf "$BUILD"' EXIT

cp "$PKGDIR"/PKGBUILD "$PKGDIR"/*.install "$BUILD/"

# Build from this checkout, not from GitHub.
sed -i "s#git+https://github.com/enfff/penna-frontend.git#git+file://$REPO#" "$BUILD/PKGBUILD"

cd "$BUILD"
makepkg "${@:--f}"

mkdir -p "$OUT"
mv -- "$BUILD"/*.pkg.tar.zst "$OUT/"
echo "package ready: $OUT"/*.pkg.tar.zst
