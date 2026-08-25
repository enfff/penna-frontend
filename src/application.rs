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

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};

use crate::about;
use crate::preferences;
use crate::PennaFrontendWindow;

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
            obj.set_accels_for_action("app.preferences", &["<control>comma"]);
            obj.set_accels_for_action("app.shortcuts", &["F1"]);
            obj.set_accels_for_action("app.quit", &["<control>q"]);
            obj.set_accels_for_action("win.back-to-grid", &["Escape"]);
            obj.set_accels_for_action("win.toggle-viewer-mode", &["<control>d"]);
            obj.set_accels_for_action("win.wrap-bold", &["<control>b"]);
            obj.set_accels_for_action("win.wrap-italic", &["<control>i"]);
            obj.set_accels_for_action("win.wrap-link", &["<control>k"]);
            obj.set_accels_for_action("win.edit-tags", &["<control>e"]);
            obj.set_accels_for_action("win.wrap-strikethrough", &["<control><shift>x"]);
            obj.set_accels_for_action("win.save", &["<control>s"]);
            obj.set_accels_for_action("win.sync-journal", &["<control>r"]);
            obj.set_accels_for_action("win.new-entry", &["<control>n"]);
            obj.set_accels_for_action("win.prefix-bullet-list", &["<control><shift>8"]);
            obj.set_accels_for_action("win.prefix-numbered-list", &["<control><shift>7"]);
            obj.set_accels_for_action("win.prefix-checklist", &["<control><shift>9"]);
            obj.set_accels_for_action("win.prefix-blockquote", &["<control><shift>period"]);
            obj.set_accels_for_action("win.wrap-code-block", &["<control><alt>e"]);
            obj.set_accels_for_action("win.conflict-accept-current", &["<Alt>c"]);
            obj.set_accels_for_action("win.conflict-accept-incoming", &["<Alt>i"]);
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
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget,
                    gio::ActionGroup, gio::ActionMap;
}

impl PennaFrontendApplication {
    pub fn new(application_id: &str, flags: &gio::ApplicationFlags) -> Self {
        glib::Object::builder()
            .property("application-id", application_id)
            .property("flags", flags)
            .property("resource-base-path", "/io/github/enfff/Diary")
            .build()
    }

    fn setup_gactions(&self) {
        let preferences_action = gio::ActionEntry::builder("preferences")
            .activate(move |app: &Self, _, _| preferences::show_preferences(app))
            .build();
        let shortcuts_action = gio::ActionEntry::builder("shortcuts")
            .activate(move |app: &Self, _, _| app.show_shortcuts())
            .build();
        let quit_action = gio::ActionEntry::builder("quit")
            .activate(move |app: &Self, _, _| app.quit())
            .build();
        let about_action = gio::ActionEntry::builder("about")
            .activate(move |app: &Self, _, _| about::show_about(app))
            .build();
        self.add_action_entries([preferences_action, shortcuts_action, quit_action, about_action]);
    }

    fn show_shortcuts(&self) {
        let Some(window) = self.active_window() else {
            return;
        };

        let builder = gtk::Builder::from_resource("/io/github/enfff/Diary/shortcuts-dialog.ui");
        let Some(shortcuts) = builder.object::<adw::ShortcutsDialog>("shortcuts_dialog") else {
            return;
        };

        shortcuts.present(Some(&window));
    }
}
