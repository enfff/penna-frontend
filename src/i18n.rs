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

pub fn saving() -> String {
    gettext("Saving…")
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

pub fn conflict_review_action() -> String {
    gettext("Review")
}

pub fn accept_current_label() -> String {
    gettext("Accept Current")
}

pub fn accept_incoming_label() -> String {
    gettext("Accept Incoming")
}

pub fn accept_both_label() -> String {
    gettext("Keep Both")
}

pub fn conflict_this_device_label() -> String {
    gettext("This device")
}

pub fn conflict_other_device_label() -> String {
    gettext("Other device")
}

pub fn conflict_keep_both_label() -> String {
    gettext("Keep both")
}

pub fn accepted_current_changes() -> String {
    gettext("Accepted current changes")
}

pub fn accepted_incoming_changes() -> String {
    gettext("Accepted incoming changes")
}

pub fn accepted_both_changes() -> String {
    gettext("Kept both versions")
}

pub fn repository_group_title() -> String {
    gettext("Repository")
}

pub fn repository_group_description() -> String {
    gettext("Choose where your diary lives, or clone one from a server. Switching reconnects the app to that diary.")
}

pub fn change_action_label() -> String {
    gettext("Change…")
}

// Kept so the msgid stays registered for translations.
#[allow(dead_code)]
pub fn repository_path_label() -> String {
    gettext("Repository path")
}

pub fn this_computer() -> String {
    gettext("This computer")
}

pub fn from_server() -> String {
    gettext("From a server")
}

pub fn clone_from_server_subtitle() -> String {
    gettext("Clone an existing diary from a repository.")
}

pub fn no_repository_connected() -> String {
    gettext("No repository connected yet")
}

pub fn choose_repository_folder() -> String {
    gettext("Choose repository folder")
}

pub fn unresolved_sync_conflict() -> String {
    gettext("Unresolved sync conflict")
}

pub fn auth_required_title() -> String {
    gettext("Authentication required")
}

pub fn auth_hint() -> String {
    gettext("Enter a personal access token for this remote. It is stored in your system keychain and reused for future syncs.")
}

pub fn token_placeholder() -> String {
    gettext("Personal access token")
}

pub fn save_token() -> String {
    gettext("Save")
}

pub fn cancel() -> String {
    gettext("Cancel")
}

pub fn clone_journal_title() -> String {
    gettext("Clone a diary")
}

pub fn clone_url_label() -> String {
    gettext("Repository URL")
}

pub fn clone_url_placeholder() -> String {
    gettext("https://")
}

pub fn clone_save_to_label() -> String {
    gettext("Save to")
}

pub fn browse_label() -> String {
    gettext("Browse…")
}

pub fn clone_action_label() -> String {
    gettext("Clone")
}

pub fn clone_action_tooltip() -> String {
    gettext("Clone a diary from a server")
}

pub fn choose_clone_destination() -> String {
    gettext("Choose a folder")
}

pub fn clone_url_required() -> String {
    gettext("Enter a repository URL.")
}

pub fn cloned_connected(url: &str) -> String {
    gettext("Cloned and connected: {}").replace("{}", url)
}

pub fn clone_failed(err: &str) -> String {
    gettext("Clone failed: {}").replace("{}", err)
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
        assert_eq!(repository_group_title(), "Repository");
        assert_eq!(
            repository_group_description(),
            "Choose where your diary lives, or clone one from a server. Switching reconnects the app to that diary."
        );
        assert_eq!(this_computer(), "This computer");
        assert_eq!(from_server(), "From a server");
        assert_eq!(
            clone_from_server_subtitle(),
            "Clone an existing diary from a repository."
        );
        assert_eq!(clone_action_tooltip(), "Clone a diary from a server");
        assert_eq!(repo_path_required(), "Repository path required");
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
        assert_eq!(accept_both_label(), "Keep Both");
        assert_eq!(conflict_keep_both_label(), "Keep both");
        assert_eq!(accepted_current_changes(), "Accepted current changes");
        assert_eq!(accepted_incoming_changes(), "Accepted incoming changes");
        assert_eq!(accepted_both_changes(), "Kept both versions");
        assert_eq!(auth_required_title(), "Authentication required");
        assert_eq!(token_placeholder(), "Personal access token");
        assert_eq!(save_token(), "Save");
        assert_eq!(cancel(), "Cancel");
        assert_eq!(clone_journal_title(), "Clone a diary");
        assert_eq!(clone_url_label(), "Repository URL");
        assert_eq!(clone_url_placeholder(), "https://");
        assert_eq!(clone_save_to_label(), "Save to");
        assert_eq!(browse_label(), "Browse…");
        assert_eq!(clone_action_label(), "Clone");
        assert_eq!(choose_clone_destination(), "Choose a folder");
        assert_eq!(clone_url_required(), "Enter a repository URL.");
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
