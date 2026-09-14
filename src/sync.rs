//! Sync boundary between the window UI and the engine.
//!
//! Every engine call the window makes is funneled through these wrappers, so
//! `window.rs` and `grid.rs` never touch the engine mutex themselves: the
//! plain wrappers take the engine mutex on the calling thread and hand the
//! plain result back; the offloaded wrappers run the engine work on a worker
//! thread (via [`offload`]) and hand the plain result back on the main
//! thread.

use std::path::PathBuf;

// `window.imp()` lives behind ObjectSubclassIsExt, which gtk4's
// subclass prelude does not re-export in this bindings generation.
use gtk::glib::subclass::prelude::ObjectSubclassIsExt;

use crate::engine::{
    ConnectResult, EngineMock, EngineOpError, EntryRecord, EntrySnapshot, EntrySummary,
    JournalHandle, JournalStatus, SyncOutcome,
};
use crate::window::PennaFrontendWindow;

fn engine_guard(window: &PennaFrontendWindow) -> std::sync::MutexGuard<'_, EngineMock> {
    window.imp().engine.lock().unwrap()
}

/// Worker primitive behind the offloaded wrappers below: runs a blocking
/// engine operation off the main thread, then hands the result back on the
/// main thread. Keeps git I/O off the UI thread so the window never freezes
/// while an operation is in flight. `work` owns the engine lock itself;
/// `done` runs on the main thread and must perform all UI updates for the
/// result.
///
/// Only the result (which is `Send`) crosses the thread boundary, via a
/// channel. `done` itself may hold a non-`Send` window weak ref because it is
/// invoked through `idle_add_local`, which keeps everything on the main thread
/// and does not require `Send`. If the window is destroyed before the result
/// lands, `done` is a no-op (its own weak ref fails to upgrade).
fn offload<T, W, D>(
    window: &PennaFrontendWindow,
    work: W,
    done: D,
) where
    T: Send + 'static,
    W: FnOnce(&std::sync::Mutex<EngineMock>) -> T + Send + 'static,
    D: FnOnce(T) + 'static,
{
    let engine = window.imp().engine.clone();
    let (tx, rx) = std::sync::mpsc::channel::<T>();
    std::thread::spawn(move || {
        let result = work(&engine);
        let _ = tx.send(result);
    });
    let mut done = Option::Some(done);
    gtk::glib::idle_add_local(move || {
        match rx.try_recv() {
            Ok(result) => {
                if let Some(finish) = done.take() {
                    finish(result);
                }
                gtk::glib::ControlFlow::Break
            }
            Err(_) => gtk::glib::ControlFlow::Continue,
        }
    });
}

pub fn connect_journal(
    window: &PennaFrontendWindow,
    repo_path: &str,
) -> Result<ConnectResult, String> {
    engine_guard(window).connect_journal(repo_path)
}

/// Clone a journal from a remote; see [`EngineMock::clone_journal`].
pub fn clone_journal(
    window: &PennaFrontendWindow,
    remote_url: &str,
    local_parent_dir: &str,
    directory_name: &str,
) -> Result<ConnectResult, EngineOpError> {
    engine_guard(window).clone_journal(remote_url, local_parent_dir, directory_name)
}

/// Latest journal status, or `None` when the handle is not connected.
pub fn journal_status(
    window: &PennaFrontendWindow,
    handle: JournalHandle,
) -> Option<JournalStatus> {
    engine_guard(window).journal_status(handle)
}

/// Fetch + push + merge through the engine (ADR 0014 flow).
///
/// Returns [`EngineOpError`] so the caller can react to a structured
/// `AuthRequired` (prompt for a credential) instead of only a message string.
pub fn sync_journal(
    window: &PennaFrontendWindow,
    handle: JournalHandle,
) -> Result<SyncOutcome, EngineOpError> {
    engine_guard(window).sync_journal(handle)
}

/// Store a credential for `remote_url` in the platform secret store.
pub fn store_credential(
    window: &PennaFrontendWindow,
    remote_url: &str,
    token: &str,
) -> Result<(), String> {
    engine_guard(window).store_credential(remote_url, token)
}

pub fn list_entries(window: &PennaFrontendWindow, handle: JournalHandle) -> Vec<EntrySummary> {
    engine_guard(window).list_entries(handle)
}

pub fn get_entry(
    window: &PennaFrontendWindow,
    handle: JournalHandle,
    entry_id: &str,
) -> Option<EntryRecord> {
    engine_guard(window).get_entry(handle, entry_id)
}

pub fn create_entry_new(
    window: &PennaFrontendWindow,
    handle: JournalHandle,
) -> Result<EntryRecord, String> {
    engine_guard(window).create_entry_new(handle)
}

pub fn delete_entry_with_snapshot(
    window: &PennaFrontendWindow,
    handle: JournalHandle,
    entry_id: &str,
) -> Result<EntrySnapshot, String> {
    engine_guard(window).delete_entry_with_snapshot(handle, entry_id)
}

pub fn restore_entry(
    window: &PennaFrontendWindow,
    handle: JournalHandle,
    snapshot: &EntrySnapshot,
) -> Result<EntryRecord, String> {
    engine_guard(window).restore_entry(handle, snapshot)
}

pub fn list_tags(window: &PennaFrontendWindow, handle: JournalHandle) -> Vec<String> {
    engine_guard(window).list_tags(handle)
}

pub fn add_tag(
    window: &PennaFrontendWindow,
    handle: JournalHandle,
    entry_id: &str,
    tag: &str,
) -> Result<Vec<String>, String> {
    engine_guard(window).add_tag(handle, entry_id, tag)
}

pub fn remove_tag(
    window: &PennaFrontendWindow,
    handle: JournalHandle,
    entry_id: &str,
    tag: &str,
) -> Result<Vec<String>, String> {
    engine_guard(window).remove_tag(handle, entry_id, tag)
}

pub fn entries_fingerprint(
    window: &PennaFrontendWindow,
    handle: JournalHandle,
) -> Result<u64, String> {
    engine_guard(window).entries_fingerprint(handle)
}

pub fn entries_directory(window: &PennaFrontendWindow, handle: JournalHandle) -> Option<PathBuf> {
    engine_guard(window).entries_directory(handle)
}

/// Everything the notes grid needs, read together in one off-main-thread pass:
/// the entry list, conflicted ids, per-entry contents, and the journal status.
pub struct GridData {
    pub entries: Vec<EntrySummary>,
    pub conflicted_ids: Vec<String>,
    pub contents: Vec<String>,
    pub status: Option<JournalStatus>,
}

/// Result of an off-main-thread save: the commit outcome plus the fresh
/// journal status, read together under a single engine lock.
pub struct SaveOutcome {
    pub result: Result<(), String>,
    pub status: Option<JournalStatus>,
}

/// Result of an off-main-thread disk reload: the reloaded entry count plus the
/// fresh fingerprint, read together under a single engine lock.
pub struct ReloadReport {
    pub count: Result<usize, String>,
    pub fingerprint: Option<u64>,
}

/// Loads everything the notes grid needs (entries, conflicted ids, contents,
/// status) on a worker thread, then hands the [`GridData`] to `done` on the
/// main thread.
pub fn grid_entry_data(
    window: &PennaFrontendWindow,
    handle: JournalHandle,
    done: impl FnOnce(GridData) + 'static,
) {
    offload(
        window,
        move |engine| {
            let engine = engine.lock().unwrap();
            let entries = engine.list_entries(handle);
            let conflicted_ids = engine.conflicted_entry_ids(handle);
            let contents = entries
                .iter()
                .map(|entry| {
                    engine
                        .get_entry(handle, &entry.entry_id)
                        .map(|record| record.content)
                        .unwrap_or_default()
                })
                .collect();
            let status = engine.journal_status(handle);
            GridData {
                entries,
                conflicted_ids,
                contents,
                status,
            }
        },
        done,
    );
}

/// Commits an entry save on a worker thread (the git commit), grabbing the
/// fresh status under the same lock, then hands the [`SaveOutcome`] to `done`
/// on the main thread.
pub fn save_entry_outcome(
    window: &PennaFrontendWindow,
    handle: JournalHandle,
    entry_id: String,
    content: String,
    tags: Vec<String>,
    done: impl FnOnce(SaveOutcome) + 'static,
) {
    offload(
        window,
        move |engine| {
            let mut engine = engine.lock().unwrap();
            let result = engine.entry_save(handle, &entry_id, &content, &tags);
            // Grab the fresh status under the same lock so the status bar
            // can update without a second main-thread git read.
            let status = engine.journal_status(handle);
            SaveOutcome { result, status }
        },
        done,
    );
}

/// Reads the entries fingerprint on a worker thread, then hands the result to
/// `done` on the main thread.
pub fn entries_fingerprint_async(
    window: &PennaFrontendWindow,
    handle: JournalHandle,
    done: impl FnOnce(Result<u64, String>) + 'static,
) {
    offload(
        window,
        move |engine| {
            let engine = engine.lock().unwrap();
            engine.entries_fingerprint(handle)
        },
        done,
    );
}

/// Reloads the entries from disk on a worker thread (our own save writes the
/// entry file and trips the monitor, so this must not run on the main
/// thread), then hands the [`ReloadReport`] to `done` on the main thread.
pub fn reload_entries(
    window: &PennaFrontendWindow,
    handle: JournalHandle,
    done: impl FnOnce(ReloadReport) + 'static,
) {
    offload(
        window,
        move |engine| {
            let mut engine = engine.lock().unwrap();
            let count = engine.reload_entries(handle);
            let fingerprint = engine.entries_fingerprint(handle).ok();
            ReloadReport { count, fingerprint }
        },
        done,
    );
}

