# Plan

One task per loop iteration. Tasks are done top-down. The agent marks `- [x]`
only when the gate command passes for that task's work.

## Active

- [x] T4: Conflict parser — pure function in `src/window.rs` (or a new module)
      that splits entry content into segments `Normal | Current | Incoming`
      based on standard git conflict markers (`<<<<<<<`, `=======`,
      `>>>>>>>`). Unit tests: single block, multiple blocks, block at
      start/end of file, missing `=======`, unclosed block, and prose that
      legitimately contains marker-like lines (require markers at line start).
- [x] T5: Conflict rendering — two new TextTags (`conflict-current`,
      `conflict-incoming`) with distinct tinted background colors applied to
      each side's line range; style the three marker lines dim. Rest of the
      buffer stays normally editable.
- [x] T6: Accept actions — per-conflict-block action (popover anchored to the
      block header line, or keyboard shortcut) offering "Accept Current" and
      "Accept Incoming"; accepting deletes the losing side's lines AND its
      marker lines, then re-parses the buffer. Hand-editing inside blocks must
      remain possible at all times.
- [x] T7: Save guard — Ctrl+S scans the buffer for remaining conflict tags;
      if any exist, show a toast ("N unresolved conflicts") and refuse to
      save. A fully resolved note saves through `entry_save` unchanged.
- [ ] T8: Sync surface — after a sync that reports failure/conflicts, detect
      which notes contain conflict markers (scan via `list_entries` +
      `get_entry`), badge those grid rows, toast the count, and verify that a
      follow-up sync completes the merge cleanly. If the engine makes this
      impossible mid-merge (see engine contract below), move T8 to Blocked
      with findings.

## Engine contract expectations (penna-engine team)

The frontend marker-based approach assumes:

1. Pull uses **merge**, never rebase; on conflict the engine leaves the repo
   in a standard conflicted state — working-tree files contain plain git
   markers (`merge.conflictStyle = merge`, NOT diff3/zdiff3, no `|||||||`
   base sections).
2. `get_entry` returns **working-tree content** (with markers) for conflicted
   files, not the clean HEAD/index version.
3. `entry_save`/`update_entry` must succeed while a merge is in progress
   (write file + stage is fine); committing the merge itself may wait.
4. After the last conflicted note is resolved, ONE call must finish the merge:
   either the next `sync_journal` detects in-progress merge and concludes it,
   or a new explicit API (e.g. `continue_sync(journal_handle)`). Frontend will
   call whichever exists first.
5. `journal_status` should expose `merge_in_progress` and ideally
   `conflicted_paths: []`; until then the frontend falls back to scanning
   entry contents for markers.
6. Modify/delete and delete/delete conflicts produce NO markers — decide the
   policy explicitly (suggested: modify wins, resurrect the file with the
   surviving side; delete/delete = drop silently) so the frontend never sees a
   half-state it cannot render.

## Blocked

<!-- The agent moves tasks here with a one-line reason when stuck. -->

## Done

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
