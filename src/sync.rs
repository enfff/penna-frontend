//! Sync boundary between the window UI and the engine.
//!
//! Every engine call the window makes is funneled through these wrappers:
//! each one takes the engine mutex, performs a single engine operation and
//! hands the plain result back, so `window.rs` only deals with UI feedback
//! for what comes out of here.

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

/// Runs a blocking engine operation off the main thread, then hands the result
/// back on the main thread. Keeps git I/O off the UI thread so the window never
/// freezes while an operation is in flight. `work` locks the engine itself;
/// `done` runs on the main thread and must perform all UI updates for the
/// result.
///
/// Only the result (which is `Send`) crosses the thread boundary, via a
/// channel. `done` itself may hold a non-`Send` window weak ref because it is
/// invoked through `idle_add_local`, which keeps everything on the main thread
/// and does not require `Send`. If the window is destroyed before the result
/// lands, `done` is a no-op (its own weak ref fails to upgrade).
pub(crate) fn offload<T, W, D>(
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

