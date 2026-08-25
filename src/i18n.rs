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

pub fn saved() -> String {
    gettext("Saved")
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

pub fn choose_repository_folder() -> String {
    gettext("Choose repository folder")
}

pub fn unresolved_sync_conflict() -> String {
    gettext("Unresolved sync conflict")
}

#[cfg(test)]
mod i18n_surface_tests {
    use super::*;

    fn c_locale() {
        gettextrs::setlocale(gettextrs::LocaleCategory::LcAll, "C");
    }

    #[test]
    fn simple_helpers_return_english_msgids() {
        c_locale();
        assert_eq!(diary_title(), "Diary");
        assert_eq!(repo_path_required(), "Repository path required");
        assert_eq!(
            repository_selected_click_connect(),
            "Repository selected. Click Connect."
        );
        assert_eq!(
            connect_repository_before_saving(),
            "Connect repository before saving"
        );
        assert_eq!(
            connect_repository_before_deleting(),
            "Connect repository before deleting"
        );
        assert_eq!(no_entry_selected(), "No entry selected");
        assert_eq!(no_note_selected(), "No note selected");
        assert_eq!(saved(), "Saved");
        assert_eq!(opened_todays_note(), "Opened today's note");
        assert_eq!(note_deleted(), "Note deleted");
        assert_eq!(undo_button_label(), "Undo");
        assert_eq!(note_restored(), "Note restored");
        assert_eq!(
            all_conflicts_resolved(),
            "All conflicts resolved — sync complete"
        );
        assert_eq!(accept_current_label(), "Accept Current");
        assert_eq!(accept_incoming_label(), "Accept Incoming");
        assert_eq!(accepted_current_changes(), "Accepted current changes");
        assert_eq!(accepted_incoming_changes(), "Accepted incoming changes");
        assert_eq!(change_repository(), "Change Repository");
    }

    #[test]
    fn placeholders_survive_substitution() {
        c_locale();
        assert_eq!(sync_failed("push rejected"), "Sync failed: push rejected");
        assert_eq!(sync_failed(""), "Sync failed: ");
        assert_eq!(unresolved_conflicts(7), "7 unresolved conflicts");
        assert_eq!(
            conflicts_pending(7),
            "7 notes need conflict resolution"
        );
    }

    #[test]
    fn unresolved_conflicts_keeps_singular_plural_split() {
        c_locale();
        assert_eq!(unresolved_conflicts(1), "1 unresolved conflict");
        assert_eq!(unresolved_conflicts(0), "0 unresolved conflicts");
        assert_eq!(unresolved_conflicts(12), "12 unresolved conflicts");
    }

    #[test]
    fn conflicts_pending_keeps_singular_plural_split() {
        c_locale();
        assert_eq!(
            conflicts_pending(1),
            "1 note needs conflict resolution"
        );
        assert_eq!(
            conflicts_pending(0),
            "0 notes need conflict resolution"
        );
        assert_eq!(
            conflicts_pending(12),
            "12 notes need conflict resolution"
        );
    }
}
