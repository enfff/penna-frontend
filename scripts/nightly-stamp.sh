#!/usr/bin/env bash
# Stamp a development version into the source tree for nightly flatpak builds.
# Bumps the patch level and appends .dev<UTC date>: 0.1.4 -> 0.1.5.dev20260828.
# Stamps both meson.build (drives src/config.rs VERSION) and the metainfo
# (drives the version shown by flatpak info / app stores).
set -euo pipefail

base=$(sed -nE "s/^ *version: '([0-9]+\.[0-9]+\.[0-9]+)',/\1/p" meson.build | head -n1)
if [[ -z "$base" ]]; then
  echo "nightly-stamp: cannot parse base version from meson.build" >&2
  exit 1
fi

major=${base%%.*}
rest=${base#*.}
minor=${rest%%.*}
patch=${rest##*.}
nightly="${major}.${minor}.$((patch + 1)).dev$(date -u +%Y%m%d)"
day=$(date -u +%Y-%m-%d)

sed -i "s/version: '${base}',/version: '${nightly}',/" meson.build
sed -i "s|<releases>|<releases>\n    <release version=\"${nightly}\" date=\"${day}\"><description><p>Nightly build of the develop branch.</p></description></release>|" \
  data/io.github.enfff.Diary.metainfo.xml.in

echo "nightly-stamp: ${base} -> ${nightly}"
