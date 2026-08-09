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
use gtk::{gdk, gio, glib};
use gtk::pango;

use std::cell::RefCell;

use crate::engine::{EngineMock, JournalHandle, SyncAction};

const SETTINGS_SCHEMA_ID: &str = "com.github.pennafe";
const SETTINGS_REPOSITORY_PATH_KEY: &str = "repository-path";
const SETTINGS_EDITOR_VIEWER_MODE_KEY: &str = "editor-viewer-mode";
const HEADER_REVEAL_HOVER_Y: f64 = 56.0;
const MAIN_PAGE_MARGIN_NORMAL: i32 = 12;
const EDITOR_FONT_SIZE_DEFAULT_PT: i32 = 14;
const EDITOR_FONT_SIZE_MIN_PT: i32 = 10;
const EDITOR_FONT_SIZE_MAX_PT: i32 = 28;
const TAG_HEADING_1: &str = "md-heading-1";
const TAG_HEADING_2: &str = "md-heading-2";
const TAG_HEADING_3: &str = "md-heading-3";
const TAG_HEADING_4: &str = "md-heading-4";
const TAG_BLOCKQUOTE: &str = "md-blockquote";
const TAG_CODE: &str = "md-code";
const TAG_CODE_BLOCK: &str = "md-code-block";
const TAG_BOLD: &str = "md-bold";
const TAG_ITALIC: &str = "md-italic";
const TAG_SYNTAX: &str = "md-syntax";
const TAG_LIST_ITEM: &str = "md-list-item";
const TAG_LINK: &str = "md-link";
const TAG_CHECKED: &str = "md-checked";
const TAG_RULE: &str = "md-rule";

struct CheckboxItem {
    marker_len: usize,
    checked: bool,
}

struct LinkMatch {
    label_len: usize,
    total_len: usize,
}

mod imp {
    use super::*;

    #[derive(Debug, gtk::CompositeTemplate)]
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
        pub editor_view: TemplateChild<gtk::TextView>,
        #[template_child]
        pub header_revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub app_header_bar: TemplateChild<adw::HeaderBar>,
        #[template_child]
        pub viewer_mode_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub main_menu_button: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub back_to_grid_button: TemplateChild<gtk::Button>,

        pub engine: RefCell<EngineMock>,
        pub current_handle: RefCell<Option<JournalHandle>>,
        pub current_entry_id: RefCell<Option<String>>,
        pub entries_monitor: RefCell<Option<gio::FileMonitor>>,
        pub refresh_source: RefCell<Option<glib::SourceId>>,
        pub last_entries_fingerprint: RefCell<Option<u64>>,
        pub in_editor_view: RefCell<bool>,
        pub header_visibility_locked: RefCell<bool>,
        pub editor_css_provider: RefCell<Option<gtk::CssProvider>>,
        pub editor_font_size_pt: RefCell<i32>,
        pub editor_viewer_mode: RefCell<bool>,
    }

    impl Default for PennaFrontendWindow {
        fn default() -> Self {
            Self {
                app_stack: TemplateChild::default(),
                setup_page: TemplateChild::default(),
                main_page: TemplateChild::default(),
                content_stack: TemplateChild::default(),
                notes_page: TemplateChild::default(),
                editor_page: TemplateChild::default(),
                repo_path_entry: TemplateChild::default(),
                choose_repo_button: TemplateChild::default(),
                connect_button: TemplateChild::default(),
                setup_status_label: TemplateChild::default(),
                sync_status_label: TemplateChild::default(),
                notes_flowbox: TemplateChild::default(),
                editor_view: TemplateChild::default(),
                header_revealer: TemplateChild::default(),
                app_header_bar: TemplateChild::default(),
                viewer_mode_button: TemplateChild::default(),
                main_menu_button: TemplateChild::default(),
                back_to_grid_button: TemplateChild::default(),
                engine: RefCell::default(),
                current_handle: RefCell::default(),
                current_entry_id: RefCell::default(),
                entries_monitor: RefCell::default(),
                refresh_source: RefCell::default(),
                last_entries_fingerprint: RefCell::default(),
                in_editor_view: RefCell::default(),
                header_visibility_locked: RefCell::default(),
                editor_css_provider: RefCell::default(),
                editor_font_size_pt: RefCell::new(EDITOR_FONT_SIZE_DEFAULT_PT),
                editor_viewer_mode: RefCell::default(),
            }
        }
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

        let zoom_in = gio::SimpleAction::new("zoom-in", None);
        zoom_in.connect_activate(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| {
                window.adjust_editor_zoom(1);
            }
        ));
        self.add_action(&zoom_in);

        let zoom_out = gio::SimpleAction::new("zoom-out", None);
        zoom_out.connect_activate(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| {
                window.adjust_editor_zoom(-1);
            }
        ));
        self.add_action(&zoom_out);

        let toggle_viewer_mode = gio::SimpleAction::new("toggle-viewer-mode", None);
        toggle_viewer_mode.connect_activate(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| {
                window.toggle_viewer_mode();
            }
        ));
        self.add_action(&toggle_viewer_mode);

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

        self.setup_editor_css();
        self.setup_editor_tags();

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

        imp.back_to_grid_button.connect_clicked(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                window.show_grid_view();
            }
        ));

        imp.viewer_mode_button.connect_clicked(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                window.toggle_viewer_mode();
            }
        ));

        imp.editor_view.buffer().connect_changed(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                window.apply_markdown_styling();
            }
        ));

        imp.main_menu_button.connect_notify_local(
            Some("active"),
            glib::clone!(
                #[weak(rename_to = window)]
                self,
                move |button, _| {
                    let imp = window.imp();
                    let active = button.is_active();
                    *imp.header_visibility_locked.borrow_mut() = active;

                    if active {
                        imp.header_revealer.set_reveal_child(true);
                    } else if *imp.in_editor_view.borrow() {
                        imp.header_revealer.set_reveal_child(false);
                    }
                }
            ),
        );

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
                if *imp.in_editor_view.borrow() && !*imp.header_visibility_locked.borrow() {
                    imp.header_revealer.set_reveal_child(false);
                }
            }
        ));
        self.add_controller(motion);

        let scroll = gtk::EventControllerScroll::new(
            gtk::EventControllerScrollFlags::VERTICAL
                | gtk::EventControllerScrollFlags::DISCRETE,
        );
        scroll.connect_scroll(glib::clone!(
            #[weak(rename_to = window)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |controller, _, dy| {
                let state = controller.current_event_state();
                if !state.contains(gdk::ModifierType::CONTROL_MASK) {
                    return glib::Propagation::Proceed;
                }

                if dy < 0.0 {
                    window.adjust_editor_zoom(1);
                } else if dy > 0.0 {
                    window.adjust_editor_zoom(-1);
                }

                glib::Propagation::Stop
            }
        ));
        imp.editor_page.add_controller(scroll);

        self.load_editor_preferences();
        self.show_grid_view();
        self.initialize_repository_state();
    }

    fn load_editor_preferences(&self) {
        let settings = gio::Settings::new(SETTINGS_SCHEMA_ID);
        let viewer_mode = settings.boolean(SETTINGS_EDITOR_VIEWER_MODE_KEY);
        *self.imp().editor_viewer_mode.borrow_mut() = viewer_mode;
        self.apply_editor_mode();
    }

    fn toggle_viewer_mode(&self) {
        let imp = self.imp();
        let next = !*imp.editor_viewer_mode.borrow();
        *imp.editor_viewer_mode.borrow_mut() = next;

        let settings = gio::Settings::new(SETTINGS_SCHEMA_ID);
        let _ = settings.set_boolean(SETTINGS_EDITOR_VIEWER_MODE_KEY, next);

        self.apply_editor_mode();
    }

    fn apply_editor_mode(&self) {
        let imp = self.imp();
        let viewer_mode = *imp.editor_viewer_mode.borrow();

        imp.editor_view.set_editable(!viewer_mode);
        imp.editor_view.set_cursor_visible(!viewer_mode);
        imp.editor_view.set_can_focus(true);
        imp.editor_view.set_can_target(!viewer_mode);
        imp.editor_view.set_cursor_from_name(if viewer_mode {
            Some("default")
        } else {
            None
        });
        imp.viewer_mode_button.set_icon_name(if viewer_mode {
            "view-conceal-symbolic"
        } else {
            "view-reveal-symbolic"
        });
        imp.viewer_mode_button.set_tooltip_text(Some(if viewer_mode {
            "Turn off viewer mode"
        } else {
            "Turn on viewer mode"
        }));
    }

    fn setup_editor_css(&self) {
        let provider = gtk::CssProvider::new();
        *self.imp().editor_css_provider.borrow_mut() = Some(provider.clone());
        self.apply_editor_css();

        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    }

    fn apply_editor_css(&self) {
        let imp = self.imp();
        let font_size = *imp.editor_font_size_pt.borrow();
        if let Some(provider) = imp.editor_css_provider.borrow().as_ref() {
            provider.load_from_string(&format!(
                ".immersive-editor, .immersive-editor text {{\
                    background-color: transparent;\
                    background-image: none;\
                    font-size: {font_size}pt;\
                }}\
                .immersive-editor {{\
                    border-radius: 0;\
                }}"
            ));
        }
    }

    fn setup_editor_tags(&self) {
        let buffer = self.imp().editor_view.buffer();
        let table = buffer.tag_table();

        let add_tag = |tag: &gtk::TextTag| {
            if table.lookup(tag.name().as_deref().unwrap_or_default()).is_none() {
                table.add(tag);
            }
        };

        add_tag(&gtk::TextTag::builder()
            .name(TAG_HEADING_1)
            .weight(700)
            .scale(1.8)
            .pixels_above_lines(12)
            .pixels_below_lines(6)
            .build());
        add_tag(&gtk::TextTag::builder()
            .name(TAG_HEADING_2)
            .weight(700)
            .scale(1.5)
            .pixels_above_lines(10)
            .pixels_below_lines(5)
            .build());
        add_tag(&gtk::TextTag::builder()
            .name(TAG_HEADING_3)
            .weight(700)
            .scale(1.25)
            .pixels_above_lines(8)
            .pixels_below_lines(4)
            .build());
        add_tag(&gtk::TextTag::builder()
            .name(TAG_HEADING_4)
            .weight(700)
            .scale(1.1)
            .pixels_above_lines(6)
            .pixels_below_lines(3)
            .build());
        add_tag(&gtk::TextTag::builder()
            .name(TAG_BLOCKQUOTE)
            .style(pango::Style::Italic)
            .left_margin(18)
            .pixels_above_lines(4)
            .pixels_below_lines(4)
            .build());
        add_tag(&gtk::TextTag::builder()
            .name(TAG_CODE)
            .family("monospace")
            .build());
        add_tag(&gtk::TextTag::builder()
            .name(TAG_CODE_BLOCK)
            .family("monospace")
            .left_margin(18)
            .right_margin(18)
            .pixels_above_lines(6)
            .pixels_below_lines(6)
            .build());
        add_tag(&gtk::TextTag::builder()
            .name(TAG_BOLD)
            .weight(700)
            .build());
        add_tag(&gtk::TextTag::builder()
            .name(TAG_ITALIC)
            .style(pango::Style::Italic)
            .build());
        add_tag(&gtk::TextTag::builder()
            .name(TAG_SYNTAX)
            .foreground_rgba(&gdk::RGBA::new(0.0, 0.0, 0.0, 0.0))
            .scale(0.01)
            .build());
        add_tag(&gtk::TextTag::builder()
            .name(TAG_LIST_ITEM)
            .left_margin(18)
            .pixels_above_lines(2)
            .pixels_below_lines(2)
            .build());
        add_tag(&gtk::TextTag::builder()
            .name(TAG_LINK)
            .underline(pango::Underline::Single)
            .build());
        add_tag(&gtk::TextTag::builder()
            .name(TAG_CHECKED)
            .strikethrough(true)
            .build());
        add_tag(&gtk::TextTag::builder()
            .name(TAG_RULE)
            .scale(0.85)
            .weight(700)
            .justification(gtk::Justification::Center)
            .pixels_above_lines(6)
            .pixels_below_lines(6)
            .build());
    }

    fn adjust_editor_zoom(&self, delta: i32) {
        let imp = self.imp();
        let next_size = (*imp.editor_font_size_pt.borrow() + delta)
            .clamp(EDITOR_FONT_SIZE_MIN_PT, EDITOR_FONT_SIZE_MAX_PT);

        if next_size == *imp.editor_font_size_pt.borrow() {
            return;
        }

        *imp.editor_font_size_pt.borrow_mut() = next_size;
        self.apply_editor_css();
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
                "Branch: {} | head: {} | dirty: {} | entries: {}",
                status.branch, status.head_commit, status.dirty, status.entry_count
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
        imp.editor_view.buffer().set_text(&content);
        self.apply_editor_mode();
        self.apply_markdown_styling();
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

    fn apply_markdown_styling(&self) {
        let imp = self.imp();
        let buffer = imp.editor_view.buffer();
        let (start, end) = buffer.bounds();
        buffer.remove_all_tags(&start, &end);

        let text = buffer.text(&start, &end, true).to_string();
        let mut line_start_offset = 0usize;
        let mut in_code_block = false;

        for line in text.lines() {
            let line_char_len = line.chars().count();
            let line_end_offset = line_start_offset + line_char_len;
            let line_trimmed = line.trim_end();

            if line_trimmed.starts_with("```") {
                in_code_block = !in_code_block;
                Self::apply_tag_by_offset(&buffer, TAG_CODE_BLOCK, line_start_offset, line_end_offset);
                Self::apply_tag_by_offset(&buffer, TAG_SYNTAX, line_start_offset, line_end_offset);
                line_start_offset = line_end_offset + 1;
                continue;
            }

            if in_code_block {
                Self::apply_tag_by_offset(&buffer, TAG_CODE_BLOCK, line_start_offset, line_end_offset);
                line_start_offset = line_end_offset + 1;
                continue;
            }

            if let Some((level, marker_len)) = Self::parse_heading(line_trimmed) {
                let tag = match level {
                    1 => TAG_HEADING_1,
                    2 => TAG_HEADING_2,
                    3 => TAG_HEADING_3,
                    _ => TAG_HEADING_4,
                };
                Self::apply_tag_by_offset(&buffer, TAG_SYNTAX, line_start_offset, line_start_offset + marker_len);
                Self::apply_tag_by_offset(&buffer, tag, line_start_offset + marker_len, line_end_offset);
            }

            if let Some(content) = line_trimmed.strip_prefix("> ") {
                let prefix_len = line_trimmed.chars().count() - content.chars().count();
                Self::apply_tag_by_offset(&buffer, TAG_SYNTAX, line_start_offset, line_start_offset + prefix_len);
                Self::apply_tag_by_offset(&buffer, TAG_BLOCKQUOTE, line_start_offset + prefix_len, line_end_offset);
            }

            if let Some(marker_len) = Self::parse_checkbox_item(line_trimmed).map(|item| item.marker_len) {
                Self::apply_tag_by_offset(&buffer, TAG_SYNTAX, line_start_offset, line_start_offset + marker_len);
                Self::apply_tag_by_offset(&buffer, TAG_LIST_ITEM, line_start_offset + marker_len, line_end_offset);
            } else if let Some(marker_len) = Self::parse_unordered_list_item(line_trimmed) {
                Self::apply_tag_by_offset(&buffer, TAG_SYNTAX, line_start_offset, line_start_offset + marker_len);
                Self::apply_tag_by_offset(&buffer, TAG_LIST_ITEM, line_start_offset + marker_len, line_end_offset);
            } else if let Some(marker_len) = Self::parse_ordered_list_item(line_trimmed) {
                Self::apply_tag_by_offset(&buffer, TAG_LIST_ITEM, line_start_offset, line_end_offset);
                Self::apply_tag_by_offset(&buffer, TAG_SYNTAX, line_start_offset, line_start_offset + marker_len);
            }

            if Self::is_horizontal_rule(line_trimmed) {
                Self::apply_tag_by_offset(&buffer, TAG_RULE, line_start_offset, line_end_offset);
            }

            Self::apply_inline_markdown_tags(&buffer, line, line_start_offset);
            line_start_offset = line_end_offset + 1;
        }
    }

    fn parse_heading(line: &str) -> Option<(usize, usize)> {
        let hashes = line.chars().take_while(|ch| *ch == '#').count();
        if hashes == 0 || hashes > 6 {
            return None;
        }

        line.get(hashes..)
            .and_then(|rest| rest.strip_prefix(' '))
            .map(|_| (hashes, hashes + 1))
    }

    fn parse_unordered_list_item(line: &str) -> Option<usize> {
        ["- ", "* ", "+ "]
            .into_iter()
            .find_map(|prefix| line.strip_prefix(prefix).map(|_| prefix.chars().count()))
    }

    fn parse_ordered_list_item(line: &str) -> Option<usize> {
        let dot_index = line.find(". ")?;
        let (number, rest) = line.split_at(dot_index);
        if number.chars().all(|ch| ch.is_ascii_digit()) {
            rest.strip_prefix(". ")
                .map(|_| line[..(dot_index + 2)].chars().count())
        } else {
            None
        }
    }

    fn parse_checkbox_item(line: &str) -> Option<CheckboxItem> {
        ["- [ ] ", "* [ ] ", "+ [ ] ", "- [x] ", "* [x] ", "+ [x] ", "- [X] ", "* [X] ", "+ [X] "]
            .into_iter()
            .find_map(|prefix| {
                line.strip_prefix(prefix).map(|_| CheckboxItem {
                    marker_len: prefix.chars().count(),
                    checked: matches!(prefix.as_bytes().get(3), Some(b'x' | b'X')),
                })
            })
    }

    fn parse_link_at(text: &str) -> Option<LinkMatch> {
        if !text.starts_with('[') {
            return None;
        }

        let close_label = text.find(']')?;
        let after_label = text.get((close_label + 1)..)?;
        if !after_label.starts_with('(') {
            return None;
        }

        let close_url_rel = after_label.find(')')?;
        if close_label <= 1 || close_url_rel <= 1 {
            return None;
        }

        let total_byte_len = close_label + 1 + close_url_rel + 1;

        Some(LinkMatch {
            label_len: text[1..close_label].chars().count(),
            total_len: text[..total_byte_len].chars().count(),
        })
    }

    fn is_horizontal_rule(line: &str) -> bool {
        let compact: String = line.chars().filter(|ch| !ch.is_whitespace()).collect();
        if compact.len() < 3 {
            return false;
        }

        let mut chars = compact.chars();
        let Some(first) = chars.next() else {
            return false;
        };

        matches!(first, '-' | '*' | '_') && chars.all(|ch| ch == first)
    }

    fn apply_inline_markdown_tags(buffer: &gtk::TextBuffer, line: &str, line_start_offset: usize) {
        let mut index = 0usize;
        let mut bold_start = None;
        let mut italic_start = None;
        let mut code_start = None;
        let positions: Vec<(usize, usize, char)> = line
            .char_indices()
            .enumerate()
            .map(|(char_index, (byte_index, ch))| (char_index, byte_index, ch))
            .collect();

        while index < positions.len() {
            let (char_index, byte_index, ch) = positions[index];

            if line[byte_index..].starts_with("**") {
                if let Some(start) = bold_start.take() {
                    Self::apply_tag_by_offset(buffer, TAG_SYNTAX, line_start_offset + start, line_start_offset + start + 2);
                    Self::apply_tag_by_offset(buffer, TAG_BOLD, line_start_offset + start + 2, line_start_offset + char_index);
                    Self::apply_tag_by_offset(buffer, TAG_SYNTAX, line_start_offset + char_index, line_start_offset + char_index + 2);
                } else {
                    bold_start = Some(char_index);
                }
                index += 2;
                continue;
            }

            if ch == '*' {
                if let Some(start) = italic_start.take() {
                    Self::apply_tag_by_offset(buffer, TAG_SYNTAX, line_start_offset + start, line_start_offset + start + 1);
                    Self::apply_tag_by_offset(buffer, TAG_ITALIC, line_start_offset + start + 1, line_start_offset + char_index);
                    Self::apply_tag_by_offset(buffer, TAG_SYNTAX, line_start_offset + char_index, line_start_offset + char_index + 1);
                } else {
                    italic_start = Some(char_index);
                }
                index += 1;
                continue;
            }

            if ch == '`' {
                if let Some(start) = code_start.take() {
                    Self::apply_tag_by_offset(buffer, TAG_SYNTAX, line_start_offset + start, line_start_offset + start + 1);
                    Self::apply_tag_by_offset(buffer, TAG_CODE, line_start_offset + start + 1, line_start_offset + char_index);
                    Self::apply_tag_by_offset(buffer, TAG_SYNTAX, line_start_offset + char_index, line_start_offset + char_index + 1);
                } else {
                    code_start = Some(char_index);
                }
                index += 1;
                continue;
            }

            if ch == '[' {
                if let Some(link) = Self::parse_link_at(&line[byte_index..]) {
                    Self::apply_tag_by_offset(buffer, TAG_SYNTAX, line_start_offset + char_index, line_start_offset + char_index + 1);
                    Self::apply_tag_by_offset(
                        buffer,
                        TAG_LINK,
                        line_start_offset + char_index + 1,
                        line_start_offset + char_index + 1 + link.label_len,
                    );
                    Self::apply_tag_by_offset(
                        buffer,
                        TAG_SYNTAX,
                        line_start_offset + char_index + 1 + link.label_len,
                        line_start_offset + char_index + link.total_len,
                    );
                    index += link.total_len;
                    continue;
                }
            }

            index += 1;
        }

        if let Some(item) = Self::parse_checkbox_item(line) {
            if item.checked {
                Self::apply_tag_by_offset(buffer, TAG_CHECKED, line_start_offset + item.marker_len, line_start_offset + line.chars().count());
            }
        }
    }

    fn apply_tag_by_offset(buffer: &gtk::TextBuffer, tag_name: &str, start_offset: usize, end_offset: usize) {
        if start_offset >= end_offset {
            return;
        }

        let Ok(start) = i32::try_from(start_offset) else {
            return;
        };
        let Ok(end) = i32::try_from(end_offset) else {
            return;
        };

        let start_iter = buffer.iter_at_offset(start);
        let end_iter = buffer.iter_at_offset(end);
        buffer.apply_tag_by_name(tag_name, &start_iter, &end_iter);
    }

    fn show_grid_view(&self) {
        let imp = self.imp();
        *imp.in_editor_view.borrow_mut() = false;
        imp.content_stack.set_visible_child(&*imp.notes_page);
        imp.app_header_bar.set_visible(true);
        imp.header_revealer.set_reveal_child(true);
        imp.main_page.set_spacing(8);
        imp.main_page.set_margin_top(MAIN_PAGE_MARGIN_NORMAL);
        imp.main_page.set_margin_bottom(MAIN_PAGE_MARGIN_NORMAL);
        imp.main_page.set_margin_start(MAIN_PAGE_MARGIN_NORMAL);
        imp.main_page.set_margin_end(MAIN_PAGE_MARGIN_NORMAL);
        imp.back_to_grid_button.set_visible(false);
    }

    fn show_setup_page(&self) {
        let imp = self.imp();
        *imp.in_editor_view.borrow_mut() = false;
        self.stop_repo_watchers();
        imp.app_stack.set_visible_child(&*imp.setup_page);
        imp.app_header_bar.set_visible(true);
        imp.header_revealer.set_reveal_child(true);
        imp.back_to_grid_button.set_visible(false);
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
        if *imp.header_visibility_locked.borrow() {
            imp.header_revealer.set_reveal_child(true);
        } else {
            imp.header_revealer.set_reveal_child(false);
        }
        imp.main_page.set_spacing(0);
        imp.main_page.set_margin_top(0);
        imp.main_page.set_margin_bottom(0);
        imp.main_page.set_margin_start(0);
        imp.main_page.set_margin_end(0);
        imp.back_to_grid_button.set_visible(true);
    }

    fn update_editor_header_reveal(&self, pointer_y: f64) {
        let imp = self.imp();
        if *imp.in_editor_view.borrow() && !*imp.header_visibility_locked.borrow() {
            imp.header_revealer
                .set_reveal_child(pointer_y <= HEADER_REVEAL_HOVER_Y);
        }
    }
}
