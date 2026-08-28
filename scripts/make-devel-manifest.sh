#!/usr/bin/env bash
# Generate the devel (nightly) flatpak manifest from the stable one:
# tracks the develop branch, stamps a .dev<date> version, builds with -Ddevel=true.
set -euo pipefail
cd "$(dirname "$0")/.."

jq '.modules[].sources += [{"type": "shell", "commands": ["bash scripts/nightly-stamp.sh"]}]
    | (.modules[].sources[] | select(.type == "git")).branch = "develop"
    | (.modules[].["config-opts"]) += ["-Ddevel=true"]' \
  io.github.enfff.Diary.json > io.github.enfff.Diary.Devel.json

echo "wrote io.github.enfff.Diary.Devel.json"
