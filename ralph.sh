#!/usr/bin/env bash
# Ralph-style agentic loop for penna-frontend.
#
# Re-runs `opencode run` with PROMPT.md until PLAN.md has no open tasks and the
# gate is green, or MAX_ITERS is exhausted. Commits each green iteration as a
# checkpoint so a derailed pass can be reverted.
#
# Usage: ./ralph.sh [MAX_ITERS]
# Env:   RALPH_AUTOCOMMIT=0   disable per-iteration checkpoint commits
#        RALPH_ALLOW_DIRTY=1  start despite uncommitted changes (not recommended)

set -uo pipefail

MAX_ITERS="${1:-20}"
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

for ((i = 1; i <= MAX_ITERS; i++)); do
  if (( $(open_tasks) == 0 )); then
    echo "no open tasks in $PLAN"
    break
  fi

  echo "=== iteration $i/$MAX_ITERS ($(open_tasks) open) ==="

  opencode run "$(cat "$PROMPT")" | tee ".ralph-iter-$i.log" || true

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

echo "all done"
