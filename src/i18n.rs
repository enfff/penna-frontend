//! Centralized user-facing message catalogue.
//!
//! Every translatable string built in Rust code lives here as a plain
//! function so `xgettext` can extract the literals reliably — `gettext()`
//! calls nested inside `glib::clone!` macro bodies are skipped by its
//! lexer. UI code calls these helpers instead of embedding raw strings.

use gettextrs::{gettext, ngettext};

pub fn diary_title() -> String {
    gettext("Diary")
}

pub fn repo_path_required() -> String {
    gettext("Repository path required")
}

pub fn repository_selected_click_connect() -> String {
    gettext("Repository selected. Click Connect.")
}

pub fn connect_repository_before_saving() -> String {
    gettext("Connect repository before saving")
}

pub fn connect_repository_before_deleting() -> String {
    gettext("Connect repository before deleting")
}

pub fn no_entry_selected() -> String {
    gettext("No entry selected")
}

pub fn no_note_selected() -> String {
    gettext("No note selected")
}

pub fn saved_via_entry_save_api() -> String {
    gettext("Saved via entry_save API")
}

pub fn opened_todays_note() -> String {
    gettext("Opened today's note")
}

pub fn note_deleted() -> String {
    gettext("Note deleted")
}

pub fn undo_button_label() -> String {
    gettext("Undo")
}

pub fn note_restored() -> String {
    gettext("Note restored")
}

pub fn all_conflicts_resolved() -> String {
    gettext("All conflicts resolved — sync complete")
}

pub fn sync_failed(err: &str) -> String {
    gettext("Sync failed: {}").replace("{}", err)
}

pub fn unresolved_conflicts(count: usize) -> String {
    ngettext(
        "{} unresolved conflict",
        "{} unresolved conflicts",
        count as u32,
    )
    .replace("{}", &count.to_string())
}

pub fn conflicts_pending(count: usize) -> String {
    ngettext(
        "{} note needs conflict resolution",
        "{} notes need conflict resolution",
        count as u32,
    )
    .replace("{}", &count.to_string())
}

pub fn accept_current_label() -> String {
    gettext("Accept Current")
}

pub fn accept_incoming_label() -> String {
    gettext("Accept Incoming")
}

pub fn accepted_current_changes() -> String {
    gettext("Accepted current changes")
}

pub fn accepted_incoming_changes() -> String {
    gettext("Accepted incoming changes")
}

pub fn change_repository() -> String {
    gettext("Change Repository")
}
