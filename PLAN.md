# Plan

One task per loop iteration. Tasks are done top-down. The agent marks `- [x]`
only when the gate command passes for that task's work.

## Active

- [x] T1: Stabilize in-flight work — the uncommitted changes in `src/window.rs`
      and `src/window.ui` (toast/undo timer feature) must build and pass
      `cargo clippy --all-targets -- -D warnings`. Finish or clean up whatever is
      half-wired so the tree is coherent.
- [x] T2: Add unit tests for entry-id parsing/formatting (`YYYYMMDDHHmm.md`)
      round-trip: parse -> format -> parse equality, plus rejects (wrong length,
      non-digits, wrong extension). Pure logic only, no GTK fixtures needed.
- [x] T3: Add unit tests for note data parsing in `src/engine.rs`, covering the
      formats exercised by the recent "data format preview / improved data
      parsing" work, including malformed input paths.

## Blocked

<!-- The agent moves tasks here with a one-line reason when stuck. -->

## Done

<!-- Completed tasks get checked above; do not delete them until a human does. -->
