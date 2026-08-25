//! Centralized access to the GSettings schema.
//!
//! Every `SETTINGS_*` key constant and every read/write of
//! `gio::Settings` goes through this module so schema ids and key names
//! are defined exactly once.

use gtk::gio;
use gtk::glib;
use gtk::prelude::SettingsExt;

const SETTINGS_SCHEMA_ID: &str = "io.github.enfff.Diary";

pub const SETTINGS_REPOSITORY_PATH_KEY: &str = "repository-path";
pub const SETTINGS_EDITOR_VIEWER_MODE_KEY: &str = "editor-viewer-mode";
pub const SETTINGS_EDITOR_FONT_PRESET_KEY: &str = "editor-font-preset";
pub const SETTINGS_EDITOR_FONT_CUSTOM_KEY: &str = "editor-font-custom";
pub const SETTINGS_ENTRY_DATETIME_FORMAT_KEY: &str = "entry-datetime-format";
pub const SETTINGS_CONFETTI_KEY: &str = "enable-confetti-mode";

fn settings() -> gio::Settings {
    gio::Settings::new(SETTINGS_SCHEMA_ID)
}

pub fn get_str(key: &str) -> String {
    settings().string(key).to_string()
}

pub fn set_str(key: &str, value: &str) -> Result<(), glib::error::BoolError> {
    settings().set_string(key, value)
}

pub fn get_bool(key: &str) -> bool {
    settings().boolean(key)
}

pub fn set_bool(key: &str, value: bool) -> Result<(), glib::error::BoolError> {
    settings().set_boolean(key, value)
}
