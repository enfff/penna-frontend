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
use gtk::{gio, glib, pango};
use chrono::Local;
use std::rc::Rc;

use crate::config::VERSION;
use crate::settings;
use crate::PennaFrontendWindow;

const EDITOR_FONT_SIZE_MIN_PT: f64 = 10.0;
const EDITOR_FONT_SIZE_MAX_PT: f64 = 28.0;
const EDITOR_FONT_SIZE_DEFAULT_PT: f64 = 14.0;
const ENTRY_DATETIME_FORMAT_DEFAULT: &str = "%Y-%m-%d";

/// Renders a sample timestamp with the given strftime format for the live
/// preview in the settings dialog. Uses the same safe `write_to` path as the
/// notes grid so an unsupported format (e.g. `%z`) degrades to the default
/// instead of panicking.
fn preview_datetime_format(fmt: &str) -> String {
    let sample = Local::now().naive_local();
    let mut buffer = String::new();
    if sample.format(fmt).write_to(&mut buffer).is_err() {
        buffer.clear();
        let _ = sample
            .format(ENTRY_DATETIME_FORMAT_DEFAULT)
            .write_to(&mut buffer);
    }
    if buffer.is_empty() {
        String::from("(invalid format)")
    } else {
        buffer
    }
}

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

        let app_window = window.clone().downcast::<PennaFrontendWindow>().ok();

        let confetti_active = settings::get_bool(settings::SETTINGS_CONFETTI_KEY);
        let font_preset = settings::get_str(settings::SETTINGS_EDITOR_FONT_PRESET_KEY);
        let custom_font = settings::get_str(settings::SETTINGS_EDITOR_FONT_CUSTOM_KEY);
        let entry_datetime_format =
            settings::get_str(settings::SETTINGS_ENTRY_DATETIME_FORMAT_KEY);

        let prefs = adw::PreferencesDialog::new();

        let page = adw::PreferencesPage::new();
        let group = adw::PreferencesGroup::builder()
            .title("General")
            .build();

        let mock_row = adw::SwitchRow::builder()
            .title("Enable confetti mode")
            // .subtitle("Mocked preference for now")
            .active(confetti_active)
            .build();

        mock_row.connect_active_notify(move |row| {
            let _ = settings::set_bool(settings::SETTINGS_CONFETTI_KEY, row.is_active());
        });

        group.add(&mock_row);


        let font_group = adw::PreferencesGroup::builder()
            .title("Editor")
            .build();
        let font_size_group = adw::PreferencesGroup::new();

        let options_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        options_box.set_hexpand(true);
        options_box.set_homogeneous(true);
        options_box.set_margin_top(12);

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
        let sans_caption = gtk::Label::new(Some("Adwaita Sans"));
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
        serif_preview.set_markup("<span font_desc=\"Free Serif Bold 42\">Ab</span>");
        serif_preview.set_justify(gtk::Justification::Center);
        serif_preview.set_halign(gtk::Align::Center);
        serif_card.append(&serif_preview);
        let serif_caption = gtk::Label::new(Some("Free Serif"));
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
        custom_preview.set_hexpand(true);
        custom_preview.set_vexpand(true);
        custom_preview.set_halign(gtk::Align::Center);
        custom_preview.set_valign(gtk::Align::Center);
        let initial_custom_family = custom_font.trim();
        let initial_custom_family = if initial_custom_family.is_empty() {
            "Sans"
        } else {
            initial_custom_family
        };
        let initial_custom_family = glib::markup_escape_text(initial_custom_family);
        custom_preview.set_markup(&format!(
            "<span font_family=\"{}\" size=\"xx-large\" weight=\"bold\">Ab</span>",
            initial_custom_family
        ));
        custom_preview.set_justify(gtk::Justification::Center);
        custom_card.append(&custom_preview);
        let initial_custom_caption = if custom_font.trim().is_empty() {
            "Custom"
        } else {
            custom_font.trim()
        };
        let custom_caption = gtk::Label::new(Some(initial_custom_caption));
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
            _ => {
                if !custom_font.trim().is_empty() {
                    custom_radio.set_active(true);
                } else {
                    sans_radio.set_active(true);
                }
            }
        }

        let family_dialog = gtk::FontDialog::builder()
            .title("Select a Font Family")
            .modal(true)
            .build();
        let family_button = gtk::FontDialogButton::new(Some(family_dialog.clone()));
        family_button.set_use_font(true);
        family_button.set_use_size(false);
        family_button.set_level(gtk::FontLevel::Family);
        family_button.set_visible(false);

        let initial_family = if custom_font.trim().is_empty() {
            "Sans"
        } else {
            custom_font.trim()
        };
        let initial_desc = {
            let mut desc = pango::FontDescription::new();
            desc.set_family(initial_family);
            desc
        };
        family_button.set_font_desc(&initial_desc);

        custom_card.append(&family_button);

        let initial_font_size = app_window
            .as_ref()
            .map(|win| win.editor_font_size_pt() as f64)
            .unwrap_or(EDITOR_FONT_SIZE_DEFAULT_PT);

        let font_size_row = adw::SpinRow::with_range(
            EDITOR_FONT_SIZE_MIN_PT,
            EDITOR_FONT_SIZE_MAX_PT,
            1.0,
        );
        font_size_row.set_title("Font size");
        font_size_row.set_digits(0);
        font_size_row.set_numeric(true);
        font_size_row.set_value(initial_font_size);

        options_box.append(&sans_frame);
        options_box.append(&serif_frame);
        options_box.append(&custom_frame);
        font_group.add(&options_box);
        font_size_group.add(&font_size_row);

        let app_for_sans = self.clone();
        sans_radio.connect_toggled(move |radio| {
            if !radio.is_active() {
                return;
            }

            let _ = settings::set_str(settings::SETTINGS_EDITOR_FONT_PRESET_KEY, "sans");

            if let Some(window) = app_for_sans.active_window() {
                if let Ok(window) = window.downcast::<PennaFrontendWindow>() {
                    window.refresh_editor_appearance();
                }
            }
        });

        let app_for_serif = self.clone();
        serif_radio.connect_toggled(move |radio| {
            if !radio.is_active() {
                return;
            }

            let _ = settings::set_str(settings::SETTINGS_EDITOR_FONT_PRESET_KEY, "serif");

            if let Some(window) = app_for_serif.active_window() {
                if let Ok(window) = window.downcast::<PennaFrontendWindow>() {
                    window.refresh_editor_appearance();
                }
            }
        });

        let open_custom_font_dialog: Rc<dyn Fn()> = {
            let family_dialog = family_dialog.clone();
            let family_button = family_button.clone();
            let window_for_dialog = window.clone();
            Rc::new(move || {
                let family_button_for_result = family_button.clone();
                family_dialog.choose_family(
                    Some(&window_for_dialog),
                    None::<&pango::FontFamily>,
                    None::<&gio::Cancellable>,
                    move |result| {
                        if let Ok(family) = result {
                            let mut desc = pango::FontDescription::new();
                            desc.set_family(&family.name());
                            family_button_for_result.set_font_desc(&desc);
                        }
                    },
                );
            })
        };

        let app_for_custom_choice = self.clone();
        let open_custom_font_dialog_for_toggle = open_custom_font_dialog.clone();
        custom_radio.connect_toggled(move |radio| {
            if !radio.is_active() {
                return;
            }

            let _ = settings::set_str(settings::SETTINGS_EDITOR_FONT_PRESET_KEY, "custom");
            open_custom_font_dialog_for_toggle();

            if let Some(window) = app_for_custom_choice.active_window() {
                if let Ok(window) = window.downcast::<PennaFrontendWindow>() {
                    window.refresh_editor_appearance();
                }
            }
        });

        let open_custom_font_dialog_for_click = open_custom_font_dialog.clone();
        let custom_radio_for_click = custom_radio.clone();
        let custom_click = gtk::GestureClick::new();
        custom_click.connect_released(move |_, _, _, _| {
            if custom_radio_for_click.is_active() {
                open_custom_font_dialog_for_click();
            }
        });
        custom_radio.add_controller(custom_click);

        let app_for_custom = self.clone();
        let custom_preview_for_family = custom_preview.clone();
        let custom_caption_for_family = custom_caption.clone();
        family_button.connect_font_desc_notify(move |button| {
            let Some(desc) = button.font_desc() else {
                return;
            };

            let family = desc
                .family()
                .map(|name| name.to_string())
                .unwrap_or_else(|| "Sans".to_string());
            let family = family.trim().to_string();
            let family = if family.is_empty() {
                "Sans".to_string()
            } else {
                family
            };

            let _ = settings::set_str(settings::SETTINGS_EDITOR_FONT_PRESET_KEY, "custom");
            let _ = settings::set_str(settings::SETTINGS_EDITOR_FONT_CUSTOM_KEY, &family);
            custom_caption_for_family.set_label(&family);

            let family_markup = glib::markup_escape_text(&family);
            custom_preview_for_family.set_markup(&format!(
                "<span font_family=\"{}\" size=\"xx-large\" weight=\"bold\">Ab</span>",
                family_markup
            ));

            if let Some(window) = app_for_custom.active_window() {
                if let Ok(window) = window.downcast::<PennaFrontendWindow>() {
                    window.refresh_editor_appearance();
                }
            }
        });

        let app_for_font_size = self.clone();
        font_size_row.connect_value_notify(move |row| {
            let size_pt = row.value() as i32;

            if let Some(window) = app_for_font_size.active_window() {
                if let Ok(window) = window.downcast::<PennaFrontendWindow>() {
                    window.set_editor_font_size_pt(size_pt);
                }
            }
        });

        let entry_format_row = adw::EntryRow::new();
        entry_format_row
            .set_title("Entry date format (docs: https://www.php.net/manual/en/function.strftime.php)");
        entry_format_row.set_text(if entry_datetime_format.trim().is_empty() {
            ENTRY_DATETIME_FORMAT_DEFAULT
        } else {
            entry_datetime_format.trim()
        });
        entry_format_row.set_show_apply_button(false);

        // Live preview of the format, shown to the right of the entry.
        let entry_format_preview = gtk::Label::new(None);
        entry_format_preview.add_css_class("dim-label");
        entry_format_preview.set_halign(gtk::Align::End);
        entry_format_row.add_suffix(&entry_format_preview);

        let initial_format = if entry_datetime_format.trim().is_empty() {
            ENTRY_DATETIME_FORMAT_DEFAULT
        } else {
            entry_datetime_format.trim()
        };
        entry_format_preview.set_text(&preview_datetime_format(initial_format));

        let app_for_entry_format = self.clone();
        entry_format_row.connect_text_notify(move |row| {
            let text = row.text();
            let normalized = if text.trim().is_empty() {
                ENTRY_DATETIME_FORMAT_DEFAULT
            } else {
                text.trim()
            };

            let _ = settings::set_str(settings::SETTINGS_ENTRY_DATETIME_FORMAT_KEY, normalized);

            entry_format_preview.set_text(&preview_datetime_format(normalized));

            if let Some(window) = app_for_entry_format.active_window() {
                if let Ok(window) = window.downcast::<PennaFrontendWindow>() {
                    window.refresh_entry_datetime_format();
                }
            }
        });

        group.add(&entry_format_row);

        let repository_group = adw::PreferencesGroup::builder()
            .title(crate::i18n::repository_group_title())
            .build();

        let current_repo_path = settings::get_str(settings::SETTINGS_REPOSITORY_PATH_KEY);

        let change_row = adw::ActionRow::builder()
            .title(crate::i18n::repository_path_label())
            .activatable(true)
            .sensitive(!current_repo_path.is_empty())
            .build();
        if !current_repo_path.is_empty() {
            change_row.set_subtitle(&current_repo_path);
        } else {
            change_row.set_subtitle(crate::i18n::no_repository_connected().as_str());
        }
        {
            let app_window = app_window.clone();
            let captured_path = current_repo_path.clone();
            // Clicking the row itself opens the journal folder.
            change_row.connect_activated(move |_| {
                let file = gio::File::for_path(&captured_path);
                gtk::FileLauncher::new(Some(&file)).open_containing_folder(
                    app_window.as_ref(),
                    None::<&gio::Cancellable>,
                    |_| {},
                );
            });
        }
        let change_button = gtk::Button::builder()
            .icon_name("folder-open-symbolic")
            .tooltip_text(crate::i18n::change_action_label())
            .valign(gtk::Align::Center)
            .css_classes(vec!["flat".to_string()])
            .build();
        if let Some(app_window) = app_window.clone() {
            change_button.connect_clicked(move |_| {
                app_window.pick_repository_and_connect();
            });
        }
        change_row.add_suffix(&change_button);
        repository_group.add(&change_row);

        page.add(&repository_group);
        page.add(&group);
        page.add(&font_group);
        page.add(&font_size_group);
        prefs.add(&page);
        prefs.present(Some(&window));
    }

    fn show_about(&self) {
        let window = self.active_window().unwrap();
        let about = adw::AboutDialog::builder()
            .application_name("Diary")
            .application_icon("io.github.enfff.Diary")
            .developer_name("Francesco P. Carmone")
            .version(VERSION)
            .developers(vec!["Francesco P. Carmone"])
            .website("https://github.com/enfff/penna-frontend")
            .issue_url("https://github.com/enfff/penna-frontend/issues")
            // Translators: Replace "translator-credits" with your name/username, and optionally an email or URL.
            .translator_credits(gettext("translator-credits"))
            .copyright("© 2026 Francesco P. Carmone")
            .build();

        about.present(Some(&window));
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
