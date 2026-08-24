#!/usr/bin/env bash
# Ralph-style agentic loop for penna-frontend.
#
# Re-runs `opencode run` with PROMPT.md until PLAN.md has no open tasks and the
# gate is green, or MAX_ITERS is exhausted. Commits each green iteration as a
# checkpoint so a derailed pass can be reverted.
#
# Usage: ./ralph.sh [MAX_ITERS]
# Env:   RALPH_MODEL=<provider/model>  model for opencode run
#                                     (default: opencode/x-preview-f-free)
#        RALPH_AUTOCOMMIT=0   disable per-iteration checkpoint commits
#        RALPH_BUMP=major|minor|patch  semver level for release on completion
#                                     (default: patch)
#        RALPH_RELEASE=0      disable version bump + tag on plan completion
#        RALPH_ALLOW_DIRTY=1  start despite uncommitted changes (not recommended)

set -uo pipefail

MAX_ITERS="${1:-20}"
MODEL="${RALPH_MODEL:-opencode/x-preview-f-free}"
PLAN="PLAN.md"
PROMPT="PROMPT.md"
PROGRESS="progress.txt"
GATE=(env CARGO_HOME=.cargo cargo clippy --all-targets -- -D warnings)

for dep in opencode cargo; do
  command -v "$dep" >/dev/null || { echo "missing dependency: $dep" >&2; exit 1; }
done
[[ -f "$PROMPT" && -f "$PLAN" ]] || { echo "need $PROMPT and $PLAN in cwd" >&2; exit 1; }

if [[ -n "$(git status --porcelain)" && "${RALPH_ALLOW_DIRTY:-0}" != "1" ]]; then
  echo "worktree is dirty — commit or stash first (or set RALPH_ALLOW_DIRTY=1)" >&2
  git status --short >&2
  exit 1
fi

gate_ok() { "${GATE[@]}" >/dev/null 2>&1; }

open_tasks() { grep -c '^- \[ \]' "$PLAN" 2>/dev/null || true; }

release_bump() {
  [[ "${RALPH_RELEASE:-1}" == "1" ]] || return 0

  local level="${RALPH_BUMP:-patch}" current major minor patch next
  current=$(sed -nE 's/^version = "(.*)"/\1/p' Cargo.toml | head -n1)
  [[ "$current" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || {
    echo "cannot parse version from Cargo.toml: '$current'" >&2
    return 1
  }
  IFS='.' read -r major minor patch <<<"$current"
  case "$level" in
    major) next="$((major + 1)).0.0" ;;
    minor) next="$major.$((minor + 1)).0" ;;
    patch) next="$major.$minor.$((patch + 1))" ;;
    *) echo "bad RALPH_BUMP '$level' (major|minor|patch)" >&2; return 1 ;;
  esac

  if git rev-parse -q --verify "refs/tags/v$next" >/dev/null; then
    echo "v$next already released — skipping bump"
    return 0
  fi

  # Keep crate, meson (feeds config.rs/about dialog) and lockfile in sync.
  sed -i "s/^version = \".*\"/version = \"$next\"/" Cargo.toml
  sed -i "s/^\([[:space:]]*\)version: '.*',/\1version: '$next',/" meson.build
  sed -i "/^name = \"penna-frontend\"$/{n;s/^version = \".*\"/version = \"$next\"/}" Cargo.lock

  env CARGO_HOME=.cargo cargo clippy --all-targets -- -D warnings >/dev/null 2>&1 || {
    echo "gate red after version bump — reverting" >&2
    git checkout -- Cargo.toml meson.build Cargo.lock
    return 1
  }

  git add Cargo.toml meson.build Cargo.lock
  git commit -q -m "chore(release): v$next"
  git tag "v$next"
  echo "released v$next (push with: git push --follow-tags)"
}

for ((i = 1; i <= MAX_ITERS; i++)); do
  if (( $(open_tasks) == 0 )); then
    echo "no open tasks in $PLAN"
    break
  fi

  echo "=== iteration $i/$MAX_ITERS ($(open_tasks) open) ==="

  opencode run -m "$MODEL" "$(cat "$PROMPT")" | tee ".ralph-iter-$i.log" || true

  if [[ ! -s ".ralph-iter-$i.log" ]] || grep -q "Error from provider" ".ralph-iter-$i.log"; then
    echo "WARNING: iteration $i produced no agent output (provider error?) — retrying next pass" >&2
    sleep 5
    continue
  fi

  if ! gate_ok; then
    echo "gate FAILED after iteration $i — next iteration should fix it" >&2
    continue
  fi

  if (( $(open_tasks) == 0 )); then
    echo "plan complete and gate green"
    break
  fi

  if [[ "${RALPH_AUTOCOMMIT:-1}" == "1" ]] && [[ -n "$(git status --porcelain)" ]]; then
    last_task=$(grep -E '^- \[x\]' "$PLAN" | tail -n1 | sed -E 's/^- \[x\] (T[0-9]+).*/\1/')
    git add -A && git commit -q -m "ralph: ${last_task:-iter-$i} [gate green]"
    echo "checkpoint committed: ${last_task:-iter-$i}"
  fi
done

if (( $(open_tasks) > 0 )); then
  echo "finished with $(open_tasks) task(s) still open — inspect progress and rerun" >&2
  exit 2
fi

release_bump
echo "all done"
