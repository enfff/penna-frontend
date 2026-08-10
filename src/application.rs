/* application.rs
 *
 * Copyright 2026 Unknown
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use gettextrs::gettext;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};

use crate::config::VERSION;
use crate::PennaFrontendWindow;

const SETTINGS_SCHEMA_ID: &str = "com.github.pennafe";
const SETTINGS_CONFETTI_KEY: &str = "enable-confetti-mode";
const SETTINGS_EDITOR_FONT_PRESET_KEY: &str = "editor-font-preset";
const SETTINGS_EDITOR_FONT_CUSTOM_KEY: &str = "editor-font-custom";

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct PennaFrontendApplication {}

    #[glib::object_subclass]
    impl ObjectSubclass for PennaFrontendApplication {
        const NAME: &'static str = "PennaFrontendApplication";
        type Type = super::PennaFrontendApplication;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for PennaFrontendApplication {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.setup_gactions();
            obj.set_accels_for_action("app.shortcuts", &["F1"]);
            obj.set_accels_for_action("app.quit", &["<control>q"]);
            obj.set_accels_for_action("win.back-to-grid", &["Escape"]);
            obj.set_accels_for_action("win.toggle-viewer-mode", &["<control>d"]);
            obj.set_accels_for_action("win.wrap-bold", &["<control>b"]);
            obj.set_accels_for_action("win.wrap-italic", &["<control>i"]);
            obj.set_accels_for_action("win.wrap-link", &["<control>k"]);
            obj.set_accels_for_action("win.wrap-code", &["<control>e"]);
            obj.set_accels_for_action("win.wrap-strikethrough", &["<control><shift>x"]);
            obj.set_accels_for_action("win.save", &["<control>s"]);
            obj.set_accels_for_action("win.prefix-bullet-list", &["<control><shift>8"]);
            obj.set_accels_for_action("win.prefix-numbered-list", &["<control><shift>7"]);
            obj.set_accels_for_action("win.prefix-checklist", &["<control><shift>9"]);
            obj.set_accels_for_action("win.prefix-blockquote", &["<control><shift>period"]);
            obj.set_accels_for_action("win.wrap-code-block", &["<control><alt>e"]);
            obj.set_accels_for_action("win.zoom-in", &["<control>plus", "<control>equal", "<control>KP_Add"]);
            obj.set_accels_for_action("win.zoom-out", &["<control>minus", "<control>KP_Subtract"]);
        }
    }

    impl ApplicationImpl for PennaFrontendApplication {
        // We connect to the activate callback to create a window when the application
        // has been launched. Additionally, this callback notifies us when the user
        // tries to launch a "second instance" of the application. When they try
        // to do that, we'll just present any existing window.
        fn activate(&self) {
            let application = self.obj();
            // Get the current window or create one if necessary
            let window = application.active_window().unwrap_or_else(|| {
                let window = PennaFrontendWindow::new(&*application);
                window.upcast()
            });

            // Ask the window manager/compositor to present the window
            window.present();
        }
    }

    impl GtkApplicationImpl for PennaFrontendApplication {}
    impl AdwApplicationImpl for PennaFrontendApplication {}
}

glib::wrapper! {
    pub struct PennaFrontendApplication(ObjectSubclass<imp::PennaFrontendApplication>)
        @extends gio::Application, gtk::Application, adw::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl PennaFrontendApplication {
    pub fn new(application_id: &str, flags: &gio::ApplicationFlags) -> Self {
        glib::Object::builder()
            .property("application-id", application_id)
            .property("flags", flags)
            .property("resource-base-path", "/com/github/pennafe")
            .build()
    }

    fn setup_gactions(&self) {
        let preferences_action = gio::ActionEntry::builder("preferences")
            .activate(move |app: &Self, _, _| app.show_preferences())
            .build();
        let shortcuts_action = gio::ActionEntry::builder("shortcuts")
            .activate(move |app: &Self, _, _| app.show_shortcuts())
            .build();
        let quit_action = gio::ActionEntry::builder("quit")
            .activate(move |app: &Self, _, _| app.quit())
            .build();
        let about_action = gio::ActionEntry::builder("about")
            .activate(move |app: &Self, _, _| app.show_about())
            .build();
        self.add_action_entries([preferences_action, shortcuts_action, quit_action, about_action]);
    }

    fn show_preferences(&self) {
        let Some(window) = self.active_window() else {
            return;
        };

        let settings = gio::Settings::new(SETTINGS_SCHEMA_ID);
        let confetti_active = settings.boolean(SETTINGS_CONFETTI_KEY);
        let font_preset = settings.string(SETTINGS_EDITOR_FONT_PRESET_KEY).to_string();
        let custom_font = settings.string(SETTINGS_EDITOR_FONT_CUSTOM_KEY).to_string();

        let prefs = adw::PreferencesDialog::new();

        let page = adw::PreferencesPage::new();
        let group = adw::PreferencesGroup::builder()
            .title("General")
            .build();

        let mock_row = adw::SwitchRow::builder()
            .title("Enable confetti mode")
            .subtitle("Mocked preference for now")
            .active(confetti_active)
            .build();

        mock_row.connect_active_notify(move |row| {
            let _ = settings.set_boolean(SETTINGS_CONFETTI_KEY, row.is_active());
        });

        group.add(&mock_row);

        let font_group = adw::PreferencesGroup::builder()
            .title("Editor")
            .build();

        let options_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        options_box.set_hexpand(true);
        options_box.set_homogeneous(true);

        let sans_radio = gtk::CheckButton::with_label("Sans");
        sans_radio.set_halign(gtk::Align::Center);
        let serif_radio = gtk::CheckButton::with_label("Serif");
        serif_radio.set_group(Some(&sans_radio));
        serif_radio.set_halign(gtk::Align::Center);
        let custom_radio = gtk::CheckButton::with_label("Custom");
        custom_radio.set_group(Some(&sans_radio));
        custom_radio.set_halign(gtk::Align::Center);

        let sans_card = gtk::Box::new(gtk::Orientation::Vertical, 12);
        sans_card.set_margin_top(14);
        sans_card.set_margin_bottom(14);
        sans_card.set_margin_start(14);
        sans_card.set_margin_end(14);
        let sans_preview = gtk::Label::new(None);
        sans_preview.set_use_markup(true);
        sans_preview.set_markup("<span font_desc=\"Adwaita Sans Bold 42\">Ab</span>");
        sans_preview.set_justify(gtk::Justification::Center);
        sans_preview.set_halign(gtk::Align::Center);
        sans_card.append(&sans_preview);
        let sans_caption = gtk::Label::new(Some("Sans (Adwaita Sans)"));
        sans_caption.add_css_class("dim-label");
        sans_caption.set_halign(gtk::Align::Center);
        sans_card.append(&sans_caption);
        sans_card.append(&sans_radio);
        let sans_frame = gtk::Frame::new(None);
        sans_frame.add_css_class("card");
        sans_frame.set_size_request(120, 100);
        sans_frame.set_child(Some(&sans_card));

        let serif_card = gtk::Box::new(gtk::Orientation::Vertical, 12);
        serif_card.set_margin_top(14);
        serif_card.set_margin_bottom(14);
        serif_card.set_margin_start(14);
        serif_card.set_margin_end(14);
        let serif_preview = gtk::Label::new(None);
        serif_preview.set_use_markup(true);
        serif_preview.set_markup("<span font_desc=\"Noto Serif Bold 42\">Ab</span>");
        serif_preview.set_justify(gtk::Justification::Center);
        serif_preview.set_halign(gtk::Align::Center);
        serif_card.append(&serif_preview);
        let serif_caption = gtk::Label::new(Some("Serif (Noto Serif)"));
        serif_caption.add_css_class("dim-label");
        serif_caption.set_halign(gtk::Align::Center);
        serif_card.append(&serif_caption);
        serif_card.append(&serif_radio);
        let serif_frame = gtk::Frame::new(None);
        serif_frame.add_css_class("card");
        serif_frame.set_size_request(120, 100);
        serif_frame.set_child(Some(&serif_card));

        let custom_card = gtk::Box::new(gtk::Orientation::Vertical, 12);
        custom_card.set_margin_top(14);
        custom_card.set_margin_bottom(14);
        custom_card.set_margin_start(14);
        custom_card.set_margin_end(14);
        let custom_preview = gtk::Label::new(None);
        custom_preview.set_use_markup(true);
        let initial_custom_family = custom_font.trim();
        let initial_custom_family = if initial_custom_family.is_empty() {
            "Sans"
        } else {
            initial_custom_family
        };
        let initial_custom_family = glib::markup_escape_text(initial_custom_family);
        custom_preview.set_markup(&format!("<span font_desc=\"{} Bold 42\">Ab</span>", initial_custom_family));
        custom_preview.set_justify(gtk::Justification::Center);
        custom_preview.set_halign(gtk::Align::Center);
        custom_card.append(&custom_preview);
        let custom_caption = gtk::Label::new(Some("Custom"));
        custom_caption.add_css_class("dim-label");
        custom_caption.set_halign(gtk::Align::Center);
        custom_card.append(&custom_caption);
        custom_card.append(&custom_radio);
        let custom_frame = gtk::Frame::new(None);
        custom_frame.add_css_class("card");
        custom_frame.set_size_request(120, 100);
        custom_frame.set_child(Some(&custom_card));

        match font_preset.as_str() {
            "custom" => custom_radio.set_active(true),
            "serif" => serif_radio.set_active(true),
            _ => sans_radio.set_active(true),
        }

        options_box.append(&sans_frame);
        options_box.append(&serif_frame);
        options_box.append(&custom_frame);
        font_group.add(&options_box);

        let custom_font_row = adw::EntryRow::builder()
            .title("Custom font family")
            .text(&custom_font)
            .show_apply_button(true)
            .build();
        custom_font_row.set_sensitive(custom_radio.is_active());
        font_group.add(&custom_font_row);

        let settings_for_sans = gio::Settings::new(SETTINGS_SCHEMA_ID);
        let app_for_sans = self.clone();
        let custom_font_row_for_sans = custom_font_row.clone();
        sans_radio.connect_toggled(move |radio| {
            if !radio.is_active() {
                return;
            }

            let _ = settings_for_sans.set_string(SETTINGS_EDITOR_FONT_PRESET_KEY, "sans");
            custom_font_row_for_sans.set_sensitive(false);

            if let Some(window) = app_for_sans.active_window() {
                if let Ok(window) = window.downcast::<PennaFrontendWindow>() {
                    window.refresh_editor_appearance();
                }
            }
        });

        let settings_for_serif = gio::Settings::new(SETTINGS_SCHEMA_ID);
        let app_for_serif = self.clone();
        let custom_font_row_for_serif = custom_font_row.clone();
        serif_radio.connect_toggled(move |radio| {
            if !radio.is_active() {
                return;
            }

            let _ = settings_for_serif.set_string(SETTINGS_EDITOR_FONT_PRESET_KEY, "serif");
            custom_font_row_for_serif.set_sensitive(false);

            if let Some(window) = app_for_serif.active_window() {
                if let Ok(window) = window.downcast::<PennaFrontendWindow>() {
                    window.refresh_editor_appearance();
                }
            }
        });

        let settings_for_custom_choice = gio::Settings::new(SETTINGS_SCHEMA_ID);
        let app_for_custom_choice = self.clone();
        let custom_font_row_for_custom = custom_font_row.clone();
        custom_radio.connect_toggled(move |radio| {
            if !radio.is_active() {
                return;
            }

            let _ = settings_for_custom_choice.set_string(SETTINGS_EDITOR_FONT_PRESET_KEY, "custom");
            custom_font_row_for_custom.set_sensitive(true);

            if let Some(window) = app_for_custom_choice.active_window() {
                if let Ok(window) = window.downcast::<PennaFrontendWindow>() {
                    window.refresh_editor_appearance();
                }
            }
        });

        let settings_for_custom = gio::Settings::new(SETTINGS_SCHEMA_ID);
        let app_for_custom = self.clone();
        let custom_preview_for_entry = custom_preview.clone();
        custom_font_row.connect_apply(move |row| {
            let _ = settings_for_custom.set_string(SETTINGS_EDITOR_FONT_CUSTOM_KEY, &row.text());

            let family = row.text();
            let family = family.trim();
            let family = if family.is_empty() { "Sans" } else { family };
            let family = glib::markup_escape_text(family);
            custom_preview_for_entry.set_markup(&format!(
                "<span font_desc=\"{} Bold 28\">Ab</span>",
                family
            ));

            if let Some(window) = app_for_custom.active_window() {
                if let Ok(window) = window.downcast::<PennaFrontendWindow>() {
                    window.refresh_editor_appearance();
                }
            }
        });

        page.add(&group);
        page.add(&font_group);
        prefs.add(&page);
        prefs.present(Some(&window));
    }

    fn show_about(&self) {
        let window = self.active_window().unwrap();
        let about = adw::AboutDialog::builder()
            .application_name("Penna-frontend")
            .application_icon("com.github.pennafe")
            .developer_name("Unknown")
            .version(VERSION)
            .developers(vec!["Unknown"])
            // Translators: Replace "translator-credits" with your name/username, and optionally an email or URL.
            .translator_credits(&gettext("translator-credits"))
            .copyright("© 2026 Unknown")
            .build();

        about.present(Some(&window));
    }

    fn show_shortcuts(&self) {
        let Some(window) = self.active_window() else {
            return;
        };

        let builder = gtk::Builder::from_resource("/com/github/pennafe/shortcuts-dialog.ui");
        let Some(shortcuts) = builder.object::<gtk::ShortcutsWindow>("shortcuts_window") else {
            return;
        };

        shortcuts.set_transient_for(Some(&window));
        shortcuts.present();
    }
}
