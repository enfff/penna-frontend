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
    ConnectResult, EngineMock, EntryRecord, EntrySnapshot, EntrySummary, JournalHandle,
    JournalStatus, SyncOutcome,
};
use crate::window::PennaFrontendWindow;

fn engine_guard(window: &PennaFrontendWindow) -> std::sync::MutexGuard<'_, EngineMock> {
    window.imp().engine.lock().unwrap()
}

pub fn connect_journal(
    window: &PennaFrontendWindow,
    repo_path: &str,
) -> Result<ConnectResult, String> {
    engine_guard(window).connect_journal(repo_path)
}

/// Latest journal status, or `None` when the handle is not connected.
pub fn journal_status(
    window: &PennaFrontendWindow,
    handle: JournalHandle,
) -> Option<JournalStatus> {
    engine_guard(window).journal_status(handle)
}

/// Fetch + push + merge through the engine (ADR 0014 flow).
pub fn sync_journal(
    window: &PennaFrontendWindow,
    handle: JournalHandle,
) -> Result<SyncOutcome, String> {
    engine_guard(window).sync_journal(handle)
}

/// Entry ids with unresolved index conflicts per the latest status.
pub fn conflicted_entry_ids(window: &PennaFrontendWindow, handle: JournalHandle) -> Vec<String> {
    engine_guard(window).conflicted_entry_ids(handle)
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

pub fn entry_save(
    window: &PennaFrontendWindow,
    handle: JournalHandle,
    entry_id: &str,
    content: &str,
    tags: &[String],
) -> Result<(), String> {
    engine_guard(window).entry_save(handle, entry_id, content, tags)
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

pub fn reload_entries(
    window: &PennaFrontendWindow,
    handle: JournalHandle,
) -> Result<usize, String> {
    engine_guard(window).reload_entries(handle)
}
