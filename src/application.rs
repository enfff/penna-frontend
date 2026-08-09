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
        let quit_action = gio::ActionEntry::builder("quit")
            .activate(move |app: &Self, _, _| app.quit())
            .build();
        let about_action = gio::ActionEntry::builder("about")
            .activate(move |app: &Self, _, _| app.show_about())
            .build();
        self.add_action_entries([preferences_action, quit_action, about_action]);
    }

    fn show_preferences(&self) {
        let Some(window) = self.active_window() else {
            return;
        };

        let settings = gio::Settings::new(SETTINGS_SCHEMA_ID);
        let confetti_active = settings.boolean(SETTINGS_CONFETTI_KEY);

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
        page.add(&group);
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
}
