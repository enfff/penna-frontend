/* window.rs
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

use gtk::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};

use std::cell::RefCell;

use crate::engine::{EngineMock, JournalHandle, SyncAction};

const SETTINGS_SCHEMA_ID: &str = "com.github.pennafe";
const SETTINGS_REPOSITORY_PATH_KEY: &str = "repository-path";
const HEADER_REVEAL_HOVER_Y: f64 = 56.0;

mod imp {
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/github/pennafe/window.ui")]
    pub struct PennaFrontendWindow {
        #[template_child]
        pub app_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub setup_page: TemplateChild<gtk::Box>,
        #[template_child]
        pub main_page: TemplateChild<gtk::Box>,
        #[template_child]
        pub content_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub notes_page: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub editor_page: TemplateChild<gtk::Box>,
        #[template_child]
        pub repo_path_entry: TemplateChild<gtk::Entry>,
        #[template_child]
        pub choose_repo_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub connect_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub setup_status_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub sync_status_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub notes_flowbox: TemplateChild<gtk::FlowBox>,
        #[template_child]
        pub entry_title_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub editor_view: TemplateChild<gtk::TextView>,
        #[template_child]
        pub header_revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub app_header_bar: TemplateChild<adw::HeaderBar>,
        #[template_child]
        pub save_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub back_to_grid_button: TemplateChild<gtk::Button>,

        pub engine: RefCell<EngineMock>,
        pub current_handle: RefCell<Option<JournalHandle>>,
        pub current_entry_id: RefCell<Option<String>>,
        pub entries_monitor: RefCell<Option<gio::FileMonitor>>,
        pub refresh_source: RefCell<Option<glib::SourceId>>,
        pub last_entries_fingerprint: RefCell<Option<u64>>,
        pub in_editor_view: RefCell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PennaFrontendWindow {
        const NAME: &'static str = "PennaFrontendWindow";
        type Type = super::PennaFrontendWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PennaFrontendWindow {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.setup_actions();
            obj.setup_callbacks();
        }
    }
    impl WidgetImpl for PennaFrontendWindow {}
    impl WindowImpl for PennaFrontendWindow {}
    impl ApplicationWindowImpl for PennaFrontendWindow {}
    impl AdwApplicationWindowImpl for PennaFrontendWindow {}
}

glib::wrapper! {
    pub struct PennaFrontendWindow(ObjectSubclass<imp::PennaFrontendWindow>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow, adw::ApplicationWindow,        @implements gio::ActionGroup, gio::ActionMap;
}

impl PennaFrontendWindow {
    pub fn new<P: IsA<gtk::Application>>(application: &P) -> Self {
        glib::Object::builder()
            .property("application", application)
            .build()
    }

    fn setup_actions(&self) {
        let save = gio::SimpleAction::new("save", None);
        save.connect_activate(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| {
                window.save_current_entry();
            }
        ));
        self.add_action(&save);

        let back_to_grid = gio::SimpleAction::new("back-to-grid", None);
        back_to_grid.connect_activate(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| {
                window.show_grid_view();
            }
        ));
        self.add_action(&back_to_grid);

        let change_repo = gio::SimpleAction::new("change-repo", None);
        change_repo.connect_activate(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| {
                window.show_setup_page();
            }
        ));
        self.add_action(&change_repo);
    }

    fn setup_callbacks(&self) {
        let imp = self.imp();

        imp.connect_button.connect_clicked(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                window.connect_journal();
            }
        ));

        imp.choose_repo_button.connect_clicked(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                window.choose_repo_folder();
            }
        ));

        imp.save_button.connect_clicked(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                window.save_current_entry();
            }
        ));

        imp.back_to_grid_button.connect_clicked(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                window.show_grid_view();
            }
        ));

        let motion = gtk::EventControllerMotion::new();
        motion.connect_motion(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _, y| {
                window.update_editor_header_reveal(y);
            }
        ));
        motion.connect_leave(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                let imp = window.imp();
                if *imp.in_editor_view.borrow() {
                    imp.header_revealer.set_reveal_child(false);
                }
            }
        ));
        self.add_controller(motion);

        self.show_grid_view();
        self.initialize_repository_state();
    }

    fn initialize_repository_state(&self) {
        let settings = gio::Settings::new(SETTINGS_SCHEMA_ID);
        let repo_path = settings.string(SETTINGS_REPOSITORY_PATH_KEY).to_string();

        if repo_path.trim().is_empty() {
            self.show_setup_page();
            return;
        }

        let imp = self.imp();
        imp.repo_path_entry.set_text(&repo_path);
        self.connect_journal();
    }

    fn connect_journal(&self) {
        let imp = self.imp();
        self.stop_entries_monitor();
        let repo_path = imp.repo_path_entry.text().trim().to_string();
        if repo_path.is_empty() {
            imp.setup_status_label.set_label("Repository path required");
            return;
        }

        let connect_result = {
            let mut engine = imp.engine.borrow_mut();
            engine.connect_journal(&repo_path)
        };

        match connect_result {
            Ok(result) => {
                *imp.current_handle.borrow_mut() = Some(result.journal_handle);

                let settings = gio::Settings::new(SETTINGS_SCHEMA_ID);
                let _ = settings.set_string(SETTINGS_REPOSITORY_PATH_KEY, &repo_path);

                let sync_message = match result.sync_action {
                    SyncAction::Downloaded => "Repository connected and downloaded",
                    SyncAction::Updated => "Repository connected and updated",
                };

                let details = format!(
                    "{sync_message} | branch: {} | capabilities: {}",
                    result.current_branch,
                    result.capabilities.join(", ")
                );
                imp.sync_status_label.set_label(&details);
                imp.setup_status_label.set_label(&details);

                self.ensure_entry_exists();
                self.refresh_notes_grid();
                self.start_repo_watchers();
                self.show_main_page();
                self.show_grid_view();
            }
            Err(err) => {
                imp.setup_status_label.set_label(&err);
                imp.sync_status_label.set_label(&err);
            }
        }
    }

    fn ensure_entry_exists(&self) {
        let imp = self.imp();
        let Some(handle) = *imp.current_handle.borrow() else {
            return;
        };

        let has_entries = {
            let engine = imp.engine.borrow();
            !engine.list_entries(handle).is_empty()
        };

        if has_entries {
            return;
        }

        let now = glib::DateTime::now_local()
            .ok()
            .and_then(|d| d.format("%Y%m%d%H%M").ok())
            .map(|s| format!("{s}.md"))
            .unwrap_or_else(|| "202601010000.md".to_string());

        let mut engine = imp.engine.borrow_mut();
        let _ = engine.create_entry(handle, &now, "");
    }

    fn refresh_notes_grid(&self) {
        let imp = self.imp();
        let Some(handle) = *imp.current_handle.borrow() else {
            return;
        };

        while let Some(child) = imp.notes_flowbox.first_child() {
            imp.notes_flowbox.remove(&child);
        }

        let entries = {
            let engine = imp.engine.borrow();
            engine.list_entries(handle)
        };

        for entry_id in &entries {
            let content = {
                let engine = imp.engine.borrow();
                engine.get_entry(handle, entry_id).unwrap_or_default()
            };

            let button = gtk::Button::new();
            button.add_css_class("card");
            button.add_css_class("flat");
            button.set_width_request(260);
            button.set_halign(gtk::Align::Start);

            let card_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
            card_box.set_margin_top(10);
            card_box.set_margin_bottom(10);
            card_box.set_margin_start(10);
            card_box.set_margin_end(10);

            let date_label = gtk::Label::new(Some(&Self::format_entry_date(entry_id)));
            date_label.set_xalign(0.0);
            date_label.add_css_class("heading");

            let preview_label = gtk::Label::new(Some(&Self::preview_text(&content)));
            preview_label.set_xalign(0.0);
            preview_label.set_wrap(true);
            preview_label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
            preview_label.set_max_width_chars(28);
            preview_label.add_css_class("dim-label");

            card_box.append(&date_label);
            card_box.append(&preview_label);
            button.set_child(Some(&card_box));

            button.connect_clicked(glib::clone!(
                #[weak(rename_to = window)]
                self,
                #[strong]
                entry_id,
                move |_| {
                    window.open_entry(entry_id.as_str());
                }
            ));
            imp.notes_flowbox.insert(&button, -1);
        }

        if let Some(status) = imp.engine.borrow().journal_status(handle) {
            let details = format!(
                "Path: {} | branch: {} | head: {} | dirty: {} | entries: {}",
                status.repo_path, status.branch, status.head_commit, status.dirty, status.entry_count
            );
            imp.sync_status_label.set_label(&details);
        }
    }

    fn open_entry(&self, entry_id: &str) {
        let imp = self.imp();
        let Some(handle) = *imp.current_handle.borrow() else {
            return;
        };

        let content = {
            let engine = imp.engine.borrow();
            engine.get_entry(handle, entry_id).unwrap_or_default()
        };

        *imp.current_entry_id.borrow_mut() = Some(entry_id.to_string());
        imp.entry_title_label.set_label(entry_id);
        imp.editor_view.buffer().set_text(&content);
        self.show_editor_view();
    }

    fn save_current_entry(&self) {
        let imp = self.imp();
        let Some(handle) = *imp.current_handle.borrow() else {
            imp.sync_status_label
                .set_label("Connect repository before saving");
            return;
        };

        let Some(entry_id) = imp.current_entry_id.borrow().clone() else {
            imp.sync_status_label.set_label("No entry selected");
            return;
        };

        let buffer = imp.editor_view.buffer();
        let (start, end) = buffer.bounds();
        let content = buffer.text(&start, &end, true).to_string();

        let save_result = {
            let mut engine = imp.engine.borrow_mut();
            engine.entry_save(handle, &entry_id, &content)
        };

        match save_result {
            Ok(()) => {
                imp.sync_status_label
                    .set_label("Saved via entry_save API");
                self.refresh_notes_grid();
                self.show_editor_view();
            }
            Err(err) => {
                imp.sync_status_label.set_label(&err);
            }
        }
    }

    fn choose_repo_folder(&self) {
        let dialog = gtk::FileDialog::builder()
            .title("Choose journal repository")
            .accept_label("Select")
            .modal(true)
            .build();

        dialog.select_folder(Some(self), None::<&gio::Cancellable>, glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |result| {
                if let Ok(file) = result {
                    if let Some(path) = file.path() {
                        let path_str = path.to_string_lossy().to_string();
                        let imp = window.imp();
                        imp.repo_path_entry.set_text(&path_str);
                        imp.setup_status_label
                            .set_label("Repository selected. Click Connect.");
                    }
                }
            }
        ));
    }

    fn start_repo_watchers(&self) {
        self.stop_repo_watchers();

        let imp = self.imp();
        let Some(handle) = *imp.current_handle.borrow() else {
            return;
        };

        let initial_fingerprint = {
            let engine = imp.engine.borrow();
            engine.entries_fingerprint(handle).ok()
        };
        *imp.last_entries_fingerprint.borrow_mut() = initial_fingerprint;

        self.start_entries_monitor();
        self.start_refresh_fallback_timer();
    }

    fn start_entries_monitor(&self) {
        let imp = self.imp();
        let Some(handle) = *imp.current_handle.borrow() else {
            return;
        };

        let watch_path = {
            let engine = imp.engine.borrow();
            engine.entries_directory(handle)
        };

        let Some(watch_path) = watch_path else {
            return;
        };

        let file = gio::File::for_path(watch_path);
        let monitor = match file.monitor_directory(gio::FileMonitorFlags::NONE, None::<&gio::Cancellable>) {
            Ok(monitor) => monitor,
            Err(err) => {
                imp.sync_status_label
                    .set_label(&format!("Unable to monitor repository files: {err}"));
                return;
            }
        };

        monitor.connect_changed(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _, _, event| {
                if matches!(
                    event,
                    gio::FileMonitorEvent::Changed
                        | gio::FileMonitorEvent::ChangesDoneHint
                        | gio::FileMonitorEvent::Created
                        | gio::FileMonitorEvent::Deleted
                        | gio::FileMonitorEvent::MovedIn
                        | gio::FileMonitorEvent::MovedOut
                ) {
                    window.reload_entries_from_disk("Detected external update.");
                }
            }
        ));

        *imp.entries_monitor.borrow_mut() = Some(monitor);
    }

    fn start_refresh_fallback_timer(&self) {
        let source_id = glib::timeout_add_seconds_local(
            2,
            glib::clone!(
                #[weak(rename_to = window)]
                self,
                #[upgrade_or]
                glib::ControlFlow::Break,
                move || {
                    window.refresh_if_repo_changed();
                    glib::ControlFlow::Continue
                }
            ),
        );

        let imp = self.imp();
        *imp.refresh_source.borrow_mut() = Some(source_id);
    }

    fn stop_repo_watchers(&self) {
        self.stop_entries_monitor();

        let imp = self.imp();
        if let Some(source) = imp.refresh_source.borrow_mut().take() {
            source.remove();
        }
        imp.last_entries_fingerprint.borrow_mut().take();
    }

    fn stop_entries_monitor(&self) {
        let imp = self.imp();
        if let Some(monitor) = imp.entries_monitor.borrow_mut().take() {
            monitor.cancel();
        }
    }

    fn refresh_if_repo_changed(&self) {
        let imp = self.imp();
        let Some(handle) = *imp.current_handle.borrow() else {
            return;
        };

        let fingerprint = {
            let engine = imp.engine.borrow();
            engine.entries_fingerprint(handle)
        };

        match fingerprint {
            Ok(current) => {
                let previous = *imp.last_entries_fingerprint.borrow();
                if previous != Some(current) {
                    self.reload_entries_from_disk("Detected external update.");
                }
            }
            Err(err) => {
                imp.sync_status_label
                    .set_label(&format!("Unable to inspect repository files: {err}"));
            }
        }
    }

    fn reload_entries_from_disk(&self, status_prefix: &str) {
        let imp = self.imp();
        let Some(handle) = *imp.current_handle.borrow() else {
            return;
        };

        let reload_result = {
            let mut engine = imp.engine.borrow_mut();
            engine.reload_entries(handle)
        };

        match reload_result {
            Ok(count) => {
                self.refresh_notes_grid();

                let new_fingerprint = {
                    let engine = imp.engine.borrow();
                    engine.entries_fingerprint(handle).ok()
                };
                *imp.last_entries_fingerprint.borrow_mut() = new_fingerprint;

                imp.sync_status_label
                    .set_label(&format!("{status_prefix} Entries: {count}"));
            }
            Err(err) => {
                imp.sync_status_label
                    .set_label(&format!("Unable to reload entries from disk: {err}"));
            }
        }
    }

    fn format_entry_date(entry_id: &str) -> String {
        if entry_id.len() < 12 {
            return entry_id.to_string();
        }

        let stem = &entry_id[..12];
        if !stem.bytes().all(|c| c.is_ascii_digit()) {
            return entry_id.to_string();
        }

        format!(
            "{}-{}-{} {}:{}",
            &stem[0..4],
            &stem[4..6],
            &stem[6..8],
            &stem[8..10],
            &stem[10..12]
        )
    }

    fn preview_text(content: &str) -> String {
        let compact = content
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        if compact.is_empty() {
            return "(Empty entry)".to_string();
        }

        let mut out = String::new();
        for ch in compact.chars().take(120) {
            out.push(ch);
        }

        if compact.chars().count() > 120 {
            out.push('…');
        }

        out
    }

    fn show_grid_view(&self) {
        let imp = self.imp();
        *imp.in_editor_view.borrow_mut() = false;
        imp.content_stack.set_visible_child(&*imp.notes_page);
        imp.app_header_bar.set_visible(true);
        imp.header_revealer.set_reveal_child(true);
        imp.back_to_grid_button.set_visible(false);
        imp.save_button.set_visible(false);
    }

    fn show_setup_page(&self) {
        let imp = self.imp();
        *imp.in_editor_view.borrow_mut() = false;
        self.stop_repo_watchers();
        imp.app_stack.set_visible_child(&*imp.setup_page);
        imp.app_header_bar.set_visible(true);
        imp.header_revealer.set_reveal_child(true);
        imp.back_to_grid_button.set_visible(false);
        imp.save_button.set_visible(false);
        imp.current_handle.borrow_mut().take();
        imp.current_entry_id.borrow_mut().take();
    }

    fn show_main_page(&self) {
        let imp = self.imp();
        imp.app_stack.set_visible_child(&*imp.main_page);
    }

    fn show_editor_view(&self) {
        let imp = self.imp();
        *imp.in_editor_view.borrow_mut() = true;
        imp.content_stack.set_visible_child(&*imp.editor_page);
        imp.app_header_bar.set_visible(true);
        imp.header_revealer.set_reveal_child(false);
        imp.back_to_grid_button.set_visible(true);
        imp.save_button.set_visible(true);
    }

    fn update_editor_header_reveal(&self, pointer_y: f64) {
        let imp = self.imp();
        if *imp.in_editor_view.borrow() {
            imp.header_revealer
                .set_reveal_child(pointer_y <= HEADER_REVEAL_HOVER_Y);
        }
    }
}
