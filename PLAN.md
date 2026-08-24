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

- [ ] T8: Sync surface — engine tag v0.1.1 makes the marker flow impossible:
      sync never starts a merge on divergence (returns `Diverged`, writes no
      markers) and `get_entry` reads HEAD content, never the working tree, so
      a marker scan via `list_entries`+`get_entry` can never find anything.
      Findings (all vs pinned checkout de4f35f = tag v0.1.1):
      1. On divergence `sync_with_mode` (adapters/git/src/git_repository.rs)
         returns `SyncResult::Diverged{branch,ahead,behind}` — fetch +
         ahead/behind only; no libgit2 merge, no MERGE_HEAD, no conflict
         markers ever written. Repos stay cleanly diverged forever.
      2. `EntryRepository::get` reads content from the HEAD commit
         (`read_file_from_commit`), ignoring working-tree files entirely —
         so even hand-written markers never surface through get_entry.
      3. `save()` always commits with a single parent; no stage-only
         mid-merge write mode.
      4. No conclude path: no `continue_sync`; a follow-up `sync_journal`
         just re-reports Diverged. "Follow-up sync completes the merge" is
         untestable — there is nothing to complete.
      5. `JournalStatus` lacks `merge_in_progress`/`conflicted_paths`.
      => all six "Engine contract expectations" above are unmet by v0.1.1;
         any badge/toast code would be dead, unfalsifiable UI.
      Unblock: unreleased engine work implementing ADR 0014 exists in the
      reference clone (vendor/penna @ v0.1.1-10-g69fa996, commit 0401e10,
      workspace version 0.2.0). Once the engine team publishes a release tag
      containing ADR 0014, bump the Cargo.toml pin and re-run T8 as written.

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
