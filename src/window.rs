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
use chrono::{format::Item, format::StrftimeItems, NaiveDateTime};

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use crate::engine::{EngineMock, JournalHandle, SyncAction};

const SETTINGS_SCHEMA_ID: &str = "com.github.pennafe";
const SETTINGS_REPOSITORY_PATH_KEY: &str = "repository-path";
const SETTINGS_EDITOR_VIEWER_MODE_KEY: &str = "editor-viewer-mode";
const SETTINGS_EDITOR_FONT_PRESET_KEY: &str = "editor-font-preset";
const SETTINGS_EDITOR_FONT_CUSTOM_KEY: &str = "editor-font-custom";
const SETTINGS_ENTRY_DATETIME_FORMAT_KEY: &str = "entry-datetime-format";
const WINDOW_TITLE_BASE: &str = "Journal";
const HEADER_REVEAL_HOVER_Y: f64 = 56.0;
const MAIN_PAGE_MARGIN_NORMAL: i32 = 12;
const NOTE_ROW_TAGS_MAX_CHARS: usize = 28;
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
const TAG_STRIKETHROUGH: &str = "md-strikethrough";
const TAG_SYNTAX: &str = "md-syntax";
const TAG_LIST_MARKER: &str = "md-list-marker";
const TAG_LIST_ITEM: &str = "md-list-item";
const TAG_LINK: &str = "md-link";
const TAG_CHECKED: &str = "md-checked";
const TAG_RULE: &str = "md-rule";
const ENTRY_DATETIME_FORMAT_DEFAULT: &str = "%Y-%m-%d";

struct CheckboxItem {
    marker_len: usize,
    checked: bool,
}

struct LinkMatch {
    label_len: usize,
    total_len: usize,
}

struct EntryTimestamp {
    value: NaiveDateTime,
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
        pub notes_search_revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub notes_search_entry: TemplateChild<gtk::SearchEntry>,
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
        pub current_entry_tags: RefCell<Vec<String>>,
        pub entries_monitor: RefCell<Option<gio::FileMonitor>>,
        pub refresh_source: RefCell<Option<glib::SourceId>>,
        pub cursor_follow_source: RefCell<Option<glib::SourceId>>,
        pub cursor_follow_late_source: RefCell<Option<glib::SourceId>>,
        pub last_entries_fingerprint: RefCell<Option<u64>>,
        pub in_editor_view: RefCell<bool>,
        pub in_notes_grid_view: RefCell<bool>,
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
                notes_search_revealer: TemplateChild::default(),
                notes_search_entry: TemplateChild::default(),
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
                current_entry_tags: RefCell::default(),
                entries_monitor: RefCell::default(),
                refresh_source: RefCell::default(),
                cursor_follow_source: RefCell::default(),
                cursor_follow_late_source: RefCell::default(),
                last_entries_fingerprint: RefCell::default(),
                in_editor_view: RefCell::default(),
                in_notes_grid_view: RefCell::default(),
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

    pub fn refresh_editor_appearance(&self) {
        self.apply_editor_css();
    }

    pub fn refresh_entry_datetime_format(&self) {
        self.refresh_notes_grid();
        let entry_id = self.imp().current_entry_id.borrow().clone();
        self.update_window_title(entry_id.as_deref());
    }

    pub fn editor_font_size_pt(&self) -> i32 {
        *self.imp().editor_font_size_pt.borrow()
    }

    pub fn set_editor_font_size_pt(&self, size_pt: i32) {
        let next_size = size_pt.clamp(EDITOR_FONT_SIZE_MIN_PT, EDITOR_FONT_SIZE_MAX_PT);

        if next_size == *self.imp().editor_font_size_pt.borrow() {
            return;
        }

        *self.imp().editor_font_size_pt.borrow_mut() = next_size;
        self.apply_editor_css();
    }

    fn set_editor_only_actions_enabled(&self, enabled: bool) {
        if let Some(action) = self
            .lookup_action("edit-tags")
            .and_then(|action| action.downcast::<gio::SimpleAction>().ok())
        {
            action.set_enabled(enabled);
        }
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

        let wrap_bold = gio::SimpleAction::new("wrap-bold", None);
        wrap_bold.connect_activate(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| {
                window.wrap_selection("**", "**");
            }
        ));
        self.add_action(&wrap_bold);

        let wrap_italic = gio::SimpleAction::new("wrap-italic", None);
        wrap_italic.connect_activate(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| {
                window.wrap_selection("*", "*");
            }
        ));
        self.add_action(&wrap_italic);

        let wrap_strikethrough = gio::SimpleAction::new("wrap-strikethrough", None);
        wrap_strikethrough.connect_activate(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| {
                window.wrap_selection("~~", "~~");
            }
        ));
        self.add_action(&wrap_strikethrough);

        let wrap_code = gio::SimpleAction::new("wrap-code", None);
        wrap_code.connect_activate(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| {
                window.wrap_selection("`", "`");
            }
        ));
        self.add_action(&wrap_code);

        let edit_tags = gio::SimpleAction::new("edit-tags", None);
        edit_tags.connect_activate(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| {
                window.open_tags_dialog();
            }
        ));
        edit_tags.set_enabled(false);
        self.add_action(&edit_tags);

        let wrap_link = gio::SimpleAction::new("wrap-link", None);
        wrap_link.connect_activate(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| {
                window.wrap_link_selection();
            }
        ));
        self.add_action(&wrap_link);

        let prefix_bullet_list = gio::SimpleAction::new("prefix-bullet-list", None);
        prefix_bullet_list.connect_activate(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| {
                window.rewrite_selected_lines(|_, line| {
                    if let Some(rest) = line
                        .strip_prefix("• ")
                        .or_else(|| line.strip_prefix("- "))
                        .or_else(|| line.strip_prefix("* "))
                        .or_else(|| line.strip_prefix("+ "))
                    {
                        rest.to_string()
                    } else {
                        format!("• {line}")
                    }
                });
            }
        ));
        self.add_action(&prefix_bullet_list);

        let prefix_numbered_list = gio::SimpleAction::new("prefix-numbered-list", None);
        prefix_numbered_list.connect_activate(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| {
                window.rewrite_selected_lines(|index, line| {
                    if let Some(rest) = window.strip_ordered_list_prefix(line) {
                        rest.to_string()
                    } else {
                        format!("{}. {line}", index + 1)
                    }
                });
            }
        ));
        self.add_action(&prefix_numbered_list);

        let prefix_checklist = gio::SimpleAction::new("prefix-checklist", None);
        prefix_checklist.connect_activate(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| {
                window.rewrite_selected_lines(|_, line| {
                    if let Some(rest) = line
                        .strip_prefix("- [ ] ")
                        .or_else(|| line.strip_prefix("* [ ] "))
                        .or_else(|| line.strip_prefix("+ [ ] "))
                        .or_else(|| line.strip_prefix("- [x] "))
                        .or_else(|| line.strip_prefix("* [x] "))
                        .or_else(|| line.strip_prefix("+ [x] "))
                        .or_else(|| line.strip_prefix("- [X] "))
                        .or_else(|| line.strip_prefix("* [X] "))
                        .or_else(|| line.strip_prefix("+ [X] "))
                    {
                        rest.to_string()
                    } else {
                        format!("- [ ] {line}")
                    }
                });
            }
        ));
        self.add_action(&prefix_checklist);

        let prefix_blockquote = gio::SimpleAction::new("prefix-blockquote", None);
        prefix_blockquote.connect_activate(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| {
                window.rewrite_selected_lines(|_, line| {
                    if let Some(rest) = line.strip_prefix("> ") {
                        rest.to_string()
                    } else {
                        format!("> {line}")
                    }
                });
            }
        ));
        self.add_action(&prefix_blockquote);

        let wrap_code_block = gio::SimpleAction::new("wrap-code-block", None);
        wrap_code_block.connect_activate(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| {
                window.insert_code_block_template();
            }
        ));
        self.add_action(&wrap_code_block);

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
                window.queue_follow_editor_cursor();
            }
        ));

        imp.editor_view.buffer().connect_mark_set(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |buffer, _, mark| {
                let insert_mark = buffer.get_insert();
                if mark != &insert_mark {
                    return;
                }

                window.queue_follow_editor_cursor();
            }
        ));

        imp.notes_search_entry.connect_search_changed(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                window.refresh_notes_grid();
                window.update_notes_search_reveal();
            }
        ));

        let typeahead = gtk::EventControllerKey::new();
        typeahead.connect_key_pressed(glib::clone!(
            #[weak(rename_to = window)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, keyval, _, state| {
                let imp = window.imp();
                if !*imp.in_notes_grid_view.borrow() || *imp.in_editor_view.borrow() {
                    return glib::Propagation::Proceed;
                }

                if imp.notes_search_entry.has_focus() {
                    return glib::Propagation::Proceed;
                }

                let disallowed = gdk::ModifierType::CONTROL_MASK
                    | gdk::ModifierType::ALT_MASK
                    | gdk::ModifierType::META_MASK
                    | gdk::ModifierType::SUPER_MASK;
                if state.intersects(disallowed) {
                    return glib::Propagation::Proceed;
                }

                let Some(ch) = keyval.to_unicode() else {
                    return glib::Propagation::Proceed;
                };

                if ch.is_control() {
                    return glib::Propagation::Proceed;
                }

                window.start_notes_search(ch);
                glib::Propagation::Stop
            }
        ));
        self.add_controller(typeahead);

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

        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        scroll.set_propagation_phase(gtk::PropagationPhase::Capture);
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
        imp.editor_view.add_controller(scroll);

        let key_controller = gtk::EventControllerKey::new();
        key_controller.connect_key_pressed(glib::clone!(
            #[weak(rename_to = window)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, keyval, _, _| {
                if matches!(keyval.to_unicode(), Some(' ')) && window.convert_leading_hyphen_to_bullet() {
                    return glib::Propagation::Stop;
                }

                if matches!(keyval.to_unicode(), Some('`')) && window.expand_code_block_from_backticks() {
                    return glib::Propagation::Stop;
                }

                if matches!(keyval, gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::ISO_Enter)
                    && window.continue_list_on_enter()
                {
                    return glib::Propagation::Stop;
                }

                if matches!(keyval, gdk::Key::Escape) {
                    window.show_grid_view();
                    return glib::Propagation::Stop;
                }

                glib::Propagation::Proceed
            }
        ));
        imp.editor_view.add_controller(key_controller);

        let grid_key_controller = gtk::EventControllerKey::new();
        grid_key_controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        grid_key_controller.connect_key_pressed(glib::clone!(
            #[weak(rename_to = window)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, keyval, _, _| {
                let imp = window.imp();
                if !*imp.in_notes_grid_view.borrow() || *imp.in_editor_view.borrow() {
                    return glib::Propagation::Proceed;
                }

                if matches!(keyval, gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::ISO_Enter) {
                    if let Some(button) = window.focused_note_button() {
                        let entry_id = button.widget_name().to_string();
                        if !entry_id.is_empty() {
                            window.open_entry(&entry_id);
                            return glib::Propagation::Stop;
                        }
                    }
                    return glib::Propagation::Proceed;
                }

                let direction = if matches!(keyval, gdk::Key::Left | gdk::Key::KP_Left) {
                    "left"
                } else if matches!(keyval, gdk::Key::Right | gdk::Key::KP_Right) {
                    "right"
                } else if matches!(keyval, gdk::Key::Up | gdk::Key::KP_Up) {
                    "up"
                } else if matches!(keyval, gdk::Key::Down | gdk::Key::KP_Down) {
                    "down"
                } else {
                    return glib::Propagation::Proceed;
                };

                if window.move_note_focus(direction) {
                    return glib::Propagation::Stop;
                }

                glib::Propagation::Proceed
            }
        ));
        imp.main_page.add_controller(grid_key_controller);

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

    fn wrap_selection(&self, prefix: &str, suffix: &str) {
        let imp = self.imp();
        if *imp.editor_viewer_mode.borrow() {
            return;
        }

        let buffer = imp.editor_view.buffer();
        let Some((mut start, mut end)) = buffer.selection_bounds() else {
            return;
        };

        if start.offset() > end.offset() {
            std::mem::swap(&mut start, &mut end);
        }

        let start_offset = start.offset();
        let selected_text = buffer.text(&start, &end, true).to_string();
        let wrapped_text = format!("{prefix}{selected_text}{suffix}");
        let selected_char_count = selected_text.chars().count() as i32;
        let prefix_char_count = prefix.chars().count() as i32;
        let suffix_char_count = suffix.chars().count() as i32;

        buffer.delete(&mut start, &mut end);

        let mut insert_iter = buffer.iter_at_offset(start_offset);
        buffer.insert(&mut insert_iter, &wrapped_text);

        let cursor_offset = start_offset + prefix_char_count + selected_char_count + suffix_char_count;
        let cursor_iter = buffer.iter_at_offset(cursor_offset);
        buffer.place_cursor(&cursor_iter);

        let mut scroll_iter = cursor_iter.clone();
        imp.editor_view.scroll_to_iter(&mut scroll_iter, 0.1, false, 0.0, 0.0);
    }

    fn wrap_link_selection(&self) {
        let imp = self.imp();
        if *imp.editor_viewer_mode.borrow() {
            return;
        }

        let buffer = imp.editor_view.buffer();
        let Some((mut start, mut end)) = buffer.selection_bounds() else {
            return;
        };

        if start.offset() > end.offset() {
            std::mem::swap(&mut start, &mut end);
        }

        let start_offset = start.offset();
        let selected_text = buffer.text(&start, &end, true).to_string();
        let wrapped_text = format!("[{selected_text}](url)");
        let selected_char_count = selected_text.chars().count() as i32;
        let url_start = start_offset + selected_char_count + 3;
        let url_end = url_start + 3;

        buffer.delete(&mut start, &mut end);

        let mut insert_iter = buffer.iter_at_offset(start_offset);
        buffer.insert(&mut insert_iter, &wrapped_text);

        let selection_start = buffer.iter_at_offset(url_start);
        let selection_end = buffer.iter_at_offset(url_end);
        buffer.select_range(&selection_start, &selection_end);
        imp.editor_view.grab_focus();
    }

    fn rewrite_selected_lines<F>(&self, mut rewrite_line: F)
    where
        F: FnMut(usize, &str) -> String,
    {
        let imp = self.imp();
        if *imp.editor_viewer_mode.borrow() {
            return;
        }

        let buffer = imp.editor_view.buffer();
        let (start_offset, end_offset, has_selection) = if let Some((mut start, mut end)) = buffer.selection_bounds() {
            if start.offset() > end.offset() {
                std::mem::swap(&mut start, &mut end);
            }
            (start.offset(), end.offset(), true)
        } else {
            let insert = buffer.iter_at_mark(&buffer.get_insert());
            let mut line_start = insert;
            line_start.set_line_offset(0);
            let mut line_end = line_start;
            line_end.forward_to_line_end();
            (line_start.offset(), line_end.offset(), false)
        };

        let mut start_iter = buffer.iter_at_offset(start_offset);
        start_iter.set_line_offset(0);
        let mut end_iter = buffer.iter_at_offset(end_offset);
        if has_selection && end_iter.line_offset() != 0 {
            end_iter.forward_to_line_end();
        }

        let replace_start = start_iter.offset();
        let replace_end = end_iter.offset();
        let selected_text = buffer.text(&start_iter, &end_iter, true).to_string();
        let replaced_text = selected_text
            .split('\n')
            .enumerate()
            .map(|(index, line)| rewrite_line(index, line))
            .collect::<Vec<_>>()
            .join("\n");
        let replaced_char_count = replaced_text.chars().count() as i32;

        let mut delete_start = buffer.iter_at_offset(replace_start);
        let mut delete_end = buffer.iter_at_offset(replace_end);
        buffer.delete(&mut delete_start, &mut delete_end);

        let mut insert_iter = buffer.iter_at_offset(replace_start);
        buffer.insert(&mut insert_iter, &replaced_text);

        let selection_start = buffer.iter_at_offset(replace_start);
        let selection_end = buffer.iter_at_offset(replace_start + replaced_char_count);
        buffer.select_range(&selection_start, &selection_end);
        imp.editor_view.grab_focus();
    }

    fn strip_ordered_list_prefix<'a>(&self, line: &'a str) -> Option<&'a str> {
        let dot_index = line.find(". ")?;
        let (number, rest) = line.split_at(dot_index);
        if number.chars().all(|ch| ch.is_ascii_digit()) {
            rest.strip_prefix(". ")
        } else {
            None
        }
    }

    fn insert_code_block_template(&self) {
        let imp = self.imp();
        if *imp.editor_viewer_mode.borrow() {
            return;
        }

        let buffer = imp.editor_view.buffer();
        let (start_offset, end_offset, selected_text) = if let Some((mut start, mut end)) = buffer.selection_bounds() {
            if start.offset() > end.offset() {
                std::mem::swap(&mut start, &mut end);
            }
            (start.offset(), end.offset(), buffer.text(&start, &end, true).to_string())
        } else {
            let insert = buffer.iter_at_mark(&buffer.get_insert());
            let mut line_start = insert;
            line_start.set_line_offset(0);
            let mut line_end = line_start;
            line_end.forward_to_line_end();
            (line_start.offset(), line_end.offset(), buffer.text(&line_start, &line_end, true).to_string())
        };

        let wrapped_text = if selected_text.is_empty() {
            "```\n\n```".to_string()
        } else {
            format!("```\n{selected_text}\n```")
        };
        let selected_char_count = selected_text.chars().count() as i32;

        let mut delete_start = buffer.iter_at_offset(start_offset);
        let mut delete_end = buffer.iter_at_offset(end_offset);
        buffer.delete(&mut delete_start, &mut delete_end);

        let mut insert_iter = buffer.iter_at_offset(start_offset);
        buffer.insert(&mut insert_iter, &wrapped_text);

        let selection_start = buffer.iter_at_offset(start_offset + 4);
        let selection_end = buffer.iter_at_offset(start_offset + 4 + selected_char_count);
        buffer.select_range(&selection_start, &selection_end);
        imp.editor_view.grab_focus();
    }

    fn continue_list_on_enter(&self) -> bool {
        let imp = self.imp();
        if *imp.editor_viewer_mode.borrow() {
            return false;
        }

        let buffer = imp.editor_view.buffer();
        if buffer.has_selection() {
            return false;
        }

        let insert = buffer.iter_at_mark(&buffer.get_insert());
        let mut line_start = insert;
        line_start.set_line_offset(0);
        let mut line_end = line_start;
        line_end.forward_to_line_end();

        let line_text = buffer.text(&line_start, &line_end, true).to_string();
        let trimmed = line_text.trim_start();
        let indent: String = line_text
            .chars()
            .take_while(|ch| matches!(ch, ' ' | '\t'))
            .collect();

        let exit_list = |buffer: &gtk::TextBuffer, line_start_offset: i32, line_end_offset: i32| {
            let mut delete_start = buffer.iter_at_offset(line_start_offset);
            let mut delete_end = buffer.iter_at_offset(line_end_offset);
            buffer.delete(&mut delete_start, &mut delete_end);
            let mut insert_iter = buffer.iter_at_offset(line_start_offset);
            buffer.insert(&mut insert_iter, "\n");
        };

        if let Some(rest) = trimmed
            .strip_prefix("- [ ] ")
            .or_else(|| trimmed.strip_prefix("* [ ] "))
            .or_else(|| trimmed.strip_prefix("+ [ ] "))
            .or_else(|| trimmed.strip_prefix("- [x] "))
            .or_else(|| trimmed.strip_prefix("* [x] "))
            .or_else(|| trimmed.strip_prefix("+ [x] "))
            .or_else(|| trimmed.strip_prefix("- [X] "))
            .or_else(|| trimmed.strip_prefix("* [X] "))
            .or_else(|| trimmed.strip_prefix("+ [X] "))
        {
            if rest.trim().is_empty() {
                exit_list(&buffer, line_start.offset(), line_end.offset());
                return true;
            }

            let mut insert_iter = buffer.iter_at_mark(&buffer.get_insert());
            buffer.insert(&mut insert_iter, &format!("\n{indent}- [ ] "));
            return true;
        }

        if let Some(rest) = trimmed
            .strip_prefix("• ")
            .or_else(|| trimmed.strip_prefix("- "))
            .or_else(|| trimmed.strip_prefix("* "))
            .or_else(|| trimmed.strip_prefix("+ "))
        {
            if rest.trim().is_empty() {
                exit_list(&buffer, line_start.offset(), line_end.offset());
                return true;
            }

            if !trimmed.starts_with("• ") {
                let marker_start = line_start.offset() + indent.chars().count() as i32;
                let mut delete_start = buffer.iter_at_offset(marker_start);
                let mut delete_end = buffer.iter_at_offset(marker_start + 1);
                buffer.delete(&mut delete_start, &mut delete_end);

                let mut insert_iter = buffer.iter_at_offset(marker_start);
                buffer.insert(&mut insert_iter, "•");
            }

            let mut insert_iter = buffer.iter_at_mark(&buffer.get_insert());
            buffer.insert(&mut insert_iter, &format!("\n{indent}• "));
            return true;
        }

        if let Some(numbered) = self.strip_ordered_list_prefix(trimmed) {
            if numbered.trim().is_empty() {
                exit_list(&buffer, line_start.offset(), line_end.offset());
                return true;
            }

            let number = trimmed
                .split('.')
                .next()
                .and_then(|raw| raw.trim().parse::<usize>().ok())
                .unwrap_or(0)
                + 1;
            let mut insert_iter = buffer.iter_at_mark(&buffer.get_insert());
            buffer.insert(&mut insert_iter, &format!("\n{indent}{number}. "));
            return true;
        }

        if let Some(rest) = trimmed.strip_prefix("> ") {
            if rest.trim().is_empty() {
                exit_list(&buffer, line_start.offset(), line_end.offset());
                return true;
            }

            let mut insert_iter = buffer.iter_at_mark(&buffer.get_insert());
            buffer.insert(&mut insert_iter, &format!("\n{indent}> "));
            return true;
        }

        false
    }

    fn convert_leading_hyphen_to_bullet(&self) -> bool {
        let imp = self.imp();
        if *imp.editor_viewer_mode.borrow() {
            return false;
        }

        let buffer = imp.editor_view.buffer();
        if buffer.has_selection() {
            return false;
        }

        let insert = buffer.iter_at_mark(&buffer.get_insert());
        let mut line_start = insert;
        line_start.set_line_offset(0);

        let line_text = buffer.text(&line_start, &insert, true).to_string();
        let indent: String = line_text
            .chars()
            .take_while(|ch| matches!(ch, ' ' | '\t'))
            .collect();
        let trimmed = line_text.trim_start();

        if trimmed != "-" {
            return false;
        }

        let marker_start = line_start.offset() + indent.chars().count() as i32;
        let mut delete_start = buffer.iter_at_offset(marker_start);
        let mut delete_end = buffer.iter_at_offset(marker_start + 1);
        buffer.delete(&mut delete_start, &mut delete_end);

        let mut insert_iter = buffer.iter_at_offset(marker_start);
        buffer.insert(&mut insert_iter, "• ");

        let cursor = buffer.iter_at_offset(marker_start + 2);
        buffer.place_cursor(&cursor);
        true
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
        let settings = gio::Settings::new(SETTINGS_SCHEMA_ID);
        let font_preset = settings.string(SETTINGS_EDITOR_FONT_PRESET_KEY).to_string();
        let custom_font = settings.string(SETTINGS_EDITOR_FONT_CUSTOM_KEY).to_string();

        let font_family_rule = match font_preset.as_str() {
            "sans" => "font-family: \"Adwaita Sans\", Sans;".to_string(),
            "serif" => "font-family: \"Free Serif\", Serif;".to_string(),
            "custom" => {
                let trimmed = custom_font.trim();
                if trimmed.is_empty() {
                    "font-family: \"Adwaita Sans\", Sans;".to_string()
                } else {
                    let escaped = trimmed.replace('"', "\\\"");
                    format!("font-family: \"{escaped}\", Sans;")
                }
            }
            _ => "font-family: \"Adwaita Sans\", Sans;".to_string(),
        };

        if let Some(provider) = imp.editor_css_provider.borrow().as_ref() {
            provider.load_from_string(&format!(
                ".immersive-editor, .immersive-editor text {{\
                    background-color: transparent;\
                    background-image: none;\
                    {font_family_rule}\
                    font-size: {font_size}pt;\
                }}\
                .immersive-editor {{\
                    border-radius: 0;\
                }}\
                .note-row {{\
                    padding: 4px 2px;\
                }}\
                .note-tags {{\
                    min-width: 0;\
                }}\
                .tag-chip {{\
                    background-color: alpha(currentColor, 0.10);\
                    border-radius: 999px;\
                    padding: 4px 10px;\
                }}\
                .tag-chip label {{\
                    font-size: 0.9em;\
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
            .line_height(1.8)
            .build());
        add_tag(&gtk::TextTag::builder()
            .name(TAG_HEADING_2)
            .weight(700)
            .scale(1.5)
            .line_height(1.6)
            .build());
        add_tag(&gtk::TextTag::builder()
            .name(TAG_HEADING_3)
            .weight(700)
            .scale(1.25)
            .line_height(1.4)
            .build());
        add_tag(&gtk::TextTag::builder()
            .name(TAG_HEADING_4)
            .weight(700)
            .scale(1.1)
            .line_height(1.4)
            .build());
        add_tag(&gtk::TextTag::builder()
            .name(TAG_BLOCKQUOTE)
            .style(pango::Style::Italic)
            .left_margin(18)
            .line_height(1.2)
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
            .line_height(1.2)
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
            .name(TAG_STRIKETHROUGH)
            .strikethrough(true)
            .build());
        add_tag(&gtk::TextTag::builder()
            .name(TAG_SYNTAX)
            .foreground_rgba(&gdk::RGBA::new(0.0, 0.0, 0.0, 0.0))
            .scale(0.01)
            .build());
        add_tag(&gtk::TextTag::builder()
            .name(TAG_LIST_MARKER)
            .foreground_rgba(&gdk::RGBA::new(0.45, 0.45, 0.45, 1.0))
            .weight(500)
            .build());
        add_tag(&gtk::TextTag::builder()
            .name(TAG_LIST_ITEM)
            .left_margin(18)
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
            .build());
    }

    fn adjust_editor_zoom(&self, delta: i32) {
        let imp = self.imp();
        let next_size = (*imp.editor_font_size_pt.borrow() + delta)
            .clamp(EDITOR_FONT_SIZE_MIN_PT, EDITOR_FONT_SIZE_MAX_PT);

        if next_size == *imp.editor_font_size_pt.borrow() {
            return;
        }

        self.set_editor_font_size_pt(next_size);
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

    fn refresh_notes_grid(&self) {
        let imp = self.imp();
        let Some(handle) = *imp.current_handle.borrow() else {
            return;
        };

        let query = imp.notes_search_entry.text().trim().to_lowercase();

        while let Some(child) = imp.notes_flowbox.first_child() {
            imp.notes_flowbox.remove(&child);
        }

        let entries = {
            let engine = imp.engine.borrow();
            engine.list_entries(handle)
        };

        let mut first_visible_button: Option<gtk::Button> = None;

        for entry in &entries {
            let content = {
                let engine = imp.engine.borrow();
                engine
                    .get_entry(handle, &entry.entry_id)
                    .map(|item| item.content)
                    .unwrap_or_default()
            };

            if !Self::entry_matches_query(&entry.entry_id, &content, &entry.tags, &query) {
                continue;
            }

            let button = gtk::Button::new();
            button.add_css_class("flat");
            button.add_css_class("note-row");
            button.set_hexpand(true);
            button.set_halign(gtk::Align::Fill);
            button.set_widget_name(&entry.entry_id);

            let row_box = gtk::CenterBox::new();
            row_box.set_margin_top(8);
            row_box.set_margin_bottom(8);
            row_box.set_margin_start(8);
            row_box.set_margin_end(8);
            row_box.set_hexpand(true);

            let note_label = gtk::Label::new(Some(&Self::format_entry_date(&entry.entry_id)));
            note_label.set_hexpand(true);
            note_label.set_halign(gtk::Align::Start);
            note_label.set_xalign(0.0);
            note_label.set_ellipsize(pango::EllipsizeMode::End);

            let tags_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            tags_box.add_css_class("note-tags");
            tags_box.set_halign(gtk::Align::End);
            tags_box.set_valign(gtk::Align::Center);
            tags_box.set_width_request(320);

            let tags_spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            tags_spacer.set_hexpand(true);

            let tags_inner = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            tags_inner.set_halign(gtk::Align::End);
            tags_inner.set_valign(gtk::Align::Center);

            tags_box.append(&tags_spacer);
            tags_box.append(&tags_inner);

            row_box.set_start_widget(Some(&note_label));
            if !entry.tags.is_empty() {
                for tag in Self::visible_tags_for_row(&entry.tags) {
                    tags_inner.append(&Self::build_tag_chip(tag));
                }
                let hidden_tags = Self::hidden_tag_count_for_row(&entry.tags);
                if hidden_tags > 0 {
                    tags_inner.append(&Self::build_tag_chip(&format!("+{hidden_tags}")));
                }
            }
            row_box.set_end_widget(Some(&tags_box));
            button.set_child(Some(&row_box));

            let entry_id = entry.entry_id.clone();
            button.connect_clicked(glib::clone!(
                #[weak(rename_to = window)]
                self,
                #[strong]
                entry_id,
                move |_| {
                    window.open_entry(entry_id.as_str());
                }
            ));
            if first_visible_button.is_none() {
                first_visible_button = Some(button.clone());
            }
            imp.notes_flowbox.insert(&button, -1);
        }

        if query.is_empty() && *imp.in_notes_grid_view.borrow() {
            if let Some(button) = first_visible_button {
                button.grab_focus();
            }
        }

        if let Some(status) = imp.engine.borrow().journal_status(handle) {
            let details = format!(
                "Branch: {} | head: {} | dirty: {} | entries: {}",
                status.branch, status.head_commit, status.dirty, status.entry_count
            );
            imp.sync_status_label.set_label(&details);
        }
    }

    fn entry_matches_query(entry_id: &str, content: &str, tags: &[String], query: &str) -> bool {
        if query.is_empty() {
            return true;
        }

        let entry_id = entry_id.to_lowercase();
        let content = content.to_lowercase();
        let tags = tags.join(" ").to_lowercase();
        entry_id.contains(query) || content.contains(query) || tags.contains(query)
    }

    fn note_buttons(&self) -> Vec<gtk::Button> {
        let imp = self.imp();
        let mut out = Vec::new();
        let mut child = imp.notes_flowbox.first_child();

        while let Some(flow_child) = child {
            if let Some(inner) = flow_child.first_child() {
                if let Ok(button) = inner.downcast::<gtk::Button>() {
                    out.push(button);
                }
            }
            child = flow_child.next_sibling();
        }

        out
    }

    fn focused_note_button(&self) -> Option<gtk::Button> {
        self.note_buttons().into_iter().find(|button| button.has_focus())
    }

    fn notes_grid_column_count(&self, total_buttons: usize, buttons: &[gtk::Button]) -> usize {
        if total_buttons <= 1 {
            return 1;
        }

        let imp = self.imp();
        let flowbox_width = imp.notes_flowbox.width();
        let column_spacing = i32::try_from(imp.notes_flowbox.column_spacing()).unwrap_or(i32::MAX);
        let max_per_line = imp.notes_flowbox.max_children_per_line().max(1) as usize;
        let sample_width = buttons.first().map(|b| b.width()).unwrap_or(0);

        if flowbox_width <= 0 || sample_width <= 0 {
            return max_per_line.min(total_buttons).max(1);
        }

        let slot = sample_width + column_spacing;
        if slot <= 0 {
            return max_per_line.min(total_buttons).max(1);
        }

        let computed = ((flowbox_width + column_spacing) / slot).max(1) as usize;
        computed.min(max_per_line).min(total_buttons).max(1)
    }

    fn move_note_focus(&self, direction: &str) -> bool {
        let buttons = self.note_buttons();
        if buttons.is_empty() {
            return false;
        }

        let current = buttons
            .iter()
            .position(|button| button.has_focus())
            .unwrap_or(0);
        let cols = self.notes_grid_column_count(buttons.len(), &buttons);
        let rows = buttons.len().div_ceil(cols);
        let current_row = current / cols;
        let current_col = current % cols;

        let target = match direction {
            "left" => {
                if current_col == 0 {
                    None
                } else {
                    Some(current - 1)
                }
            }
            "right" => {
                let next = current + 1;
                if next < buttons.len() && (next / cols) == current_row {
                    Some(next)
                } else {
                    None
                }
            }
            "up" => {
                if current_row == 0 {
                    None
                } else {
                    Some(current - cols)
                }
            }
            "down" => {
                if current_row + 1 >= rows {
                    None
                } else {
                    let next = current + cols;
                    if next < buttons.len() {
                        Some(next)
                    } else {
                        // Last row may be short: land on its last item.
                        Some(buttons.len() - 1)
                    }
                }
            }
            _ => None,
        };

        if let Some(target_idx) = target {
            if let Some(button) = buttons.get(target_idx) {
                button.grab_focus();
                return true;
            }
        }

        false
    }

    fn start_notes_search(&self, ch: char) {
        let imp = self.imp();
        if *imp.in_editor_view.borrow() || !*imp.in_notes_grid_view.borrow() {
            return;
        }

        let mut text = imp.notes_search_entry.text().to_string();
        text.push(ch);
        imp.notes_search_revealer.set_reveal_child(true);
        imp.notes_search_entry.set_text(&text);
        imp.notes_search_entry.set_position(text.chars().count() as i32);
        imp.notes_search_entry.grab_focus();
    }

    fn update_notes_search_reveal(&self) {
        let imp = self.imp();
        let reveal = *imp.in_notes_grid_view.borrow()
            && !*imp.in_editor_view.borrow()
            && !imp.notes_search_entry.text().trim().is_empty();
        imp.notes_search_revealer.set_reveal_child(reveal);
    }

    fn follow_editor_cursor_now(&self) {
        let imp = self.imp();
        if !*imp.in_editor_view.borrow() || *imp.editor_viewer_mode.borrow() {
            return;
        }

        let buffer = imp.editor_view.buffer();
        let insert_mark = buffer.get_insert();
        let mut iter = buffer.iter_at_mark(&insert_mark);

        imp.editor_view.scroll_mark_onscreen(&insert_mark);
        imp.editor_view
            .scroll_to_mark(&insert_mark, 0.0, true, 0.0, 0.97);
        imp.editor_view
            .scroll_to_iter(&mut iter, 0.0, true, 0.0, 0.97);
    }

    fn queue_follow_editor_cursor(&self) {
        let imp = self.imp();
        if imp.cursor_follow_source.borrow().is_some() {
            return;
        }

        let source_id = glib::timeout_add_local(
            Duration::from_millis(16),
            glib::clone!(
            #[weak(rename_to = window)]
            self,
            #[upgrade_or]
            glib::ControlFlow::Break,
            move || {
                window.imp().cursor_follow_source.borrow_mut().take();
                window.follow_editor_cursor_now();
                glib::ControlFlow::Break
            }
        ));

        *imp.cursor_follow_source.borrow_mut() = Some(source_id);

        if imp.cursor_follow_late_source.borrow().is_none() {
            let late_source = glib::timeout_add_local(
                Duration::from_millis(64),
                glib::clone!(
                #[weak(rename_to = window)]
                self,
                #[upgrade_or]
                glib::ControlFlow::Break,
                move || {
                    window.imp().cursor_follow_late_source.borrow_mut().take();
                    window.follow_editor_cursor_now();
                    glib::ControlFlow::Break
                }
            ));

            *imp.cursor_follow_late_source.borrow_mut() = Some(late_source);
        }
    }

    fn open_entry(&self, entry_id: &str) {
        let imp = self.imp();
        let Some(handle) = *imp.current_handle.borrow() else {
            return;
        };

        let content = {
            let engine = imp.engine.borrow();
            engine.get_entry(handle, entry_id)
        };

        let Some(entry) = content else {
            return;
        };

        *imp.current_entry_id.borrow_mut() = Some(entry_id.to_string());
        *imp.current_entry_tags.borrow_mut() = entry.tags.clone();
        self.update_window_title(Some(entry_id));
        imp.editor_view.buffer().set_text(&entry.content);
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
        let tags = imp.current_entry_tags.borrow().clone();

        let save_result = {
            let mut engine = imp.engine.borrow_mut();
            engine.entry_save(handle, &entry_id, &content, &tags)
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
        if let Some(source) = imp.cursor_follow_source.borrow_mut().take() {
            source.remove();
        }
        if let Some(source) = imp.cursor_follow_late_source.borrow_mut().take() {
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
        let Some(timestamp) = Self::parse_entry_timestamp(entry_id) else {
            return entry_id.to_string();
        };

        let fmt = Self::effective_entry_datetime_format();
        timestamp.value.format(&fmt).to_string()
    }

    fn parse_entry_timestamp(entry_id: &str) -> Option<EntryTimestamp> {
        let stem = entry_id.strip_suffix(".md").unwrap_or(entry_id);
        if stem.len() < 12 {
            return None;
        }

        let timestamp = NaiveDateTime::parse_from_str(&stem[..12], "%Y%m%d%H%M").ok()?;
        Some(EntryTimestamp { value: timestamp })
    }

    fn effective_entry_datetime_format() -> String {
        let settings = gio::Settings::new(SETTINGS_SCHEMA_ID);
        let raw = settings.string(SETTINGS_ENTRY_DATETIME_FORMAT_KEY).to_string();
        let candidate = if raw.trim().is_empty() {
            ENTRY_DATETIME_FORMAT_DEFAULT
        } else {
            raw.trim()
        };

        if Self::is_valid_chrono_format(candidate) {
            candidate.to_string()
        } else {
            ENTRY_DATETIME_FORMAT_DEFAULT.to_string()
        }
    }

    fn is_valid_chrono_format(format: &str) -> bool {
        !StrftimeItems::new(format).any(|item| matches!(item, Item::Error))
    }

    fn update_window_title(&self, entry_id: Option<&str>) {
        let title = entry_id
            .map(Self::format_entry_date)
            .filter(|text| !text.is_empty())
            .map(|text| format!("{text}"))
            .unwrap_or_else(|| WINDOW_TITLE_BASE.to_string());

        self.set_title(Some(&title));
    }

    fn visible_tags_for_row(tags: &[String]) -> Vec<&str> {
        let mut visible = Vec::new();
        let mut used_chars = 0usize;

        for tag in tags {
            let tag_chars = tag.chars().count();
            let next_cost = if visible.is_empty() { tag_chars } else { tag_chars + 1 };

            if !visible.is_empty() && used_chars + next_cost > NOTE_ROW_TAGS_MAX_CHARS {
                break;
            }

            visible.push(tag.as_str());
            used_chars += next_cost;
        }

        if visible.is_empty() && !tags.is_empty() {
            visible.push(tags[0].as_str());
        }

        visible
    }

    fn hidden_tag_count_for_row(tags: &[String]) -> usize {
        tags.len().saturating_sub(Self::visible_tags_for_row(tags).len())
    }

    fn build_tag_chip(tag: &str) -> gtk::Box {
        let chip = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        chip.add_css_class("tag-chip");

        let label = gtk::Label::new(Some(tag));
        label.add_css_class("caption");
        chip.append(&label);

        chip
    }

    fn open_tags_dialog(&self) {
        let imp = self.imp();
        let Some(handle) = *imp.current_handle.borrow() else {
            return;
        };
        let Some(entry_id) = imp.current_entry_id.borrow().clone() else {
            return;
        };

        let dialog = gtk::Window::builder()
            .transient_for(self)
            .modal(true)
            .title("Tags")
            .resizable(true)
            .build();

        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.set_margin_start(12);
        content.set_margin_end(12);
        dialog.set_child(Some(&content));

        let search_entry = gtk::SearchEntry::new();
        search_entry.set_placeholder_text(Some("Type to filter or create tag"));
        content.append(&search_entry);

        let scrolled = gtk::ScrolledWindow::new();
        scrolled.set_hscrollbar_policy(gtk::PolicyType::Never);
        scrolled.set_vscrollbar_policy(gtk::PolicyType::Automatic);
        scrolled.set_propagate_natural_height(true);
        scrolled.set_max_content_height(320);
        scrolled.set_min_content_height(180);
        scrolled.set_max_content_width(360);
        scrolled.set_min_content_width(320);

        let list_box = gtk::ListBox::new();
        list_box.add_css_class("boxed-list");
        list_box.set_selection_mode(gtk::SelectionMode::Single);
        scrolled.set_child(Some(&list_box));
        content.append(&scrolled);

        let available_tags = Rc::new(RefCell::new({
            let engine = imp.engine.borrow();
            engine.list_tags(handle)
        }));
        let current_tags = Rc::new(RefCell::new(imp.current_entry_tags.borrow().clone()));
        let render_rows_handle: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::new(RefCell::new(None));

        let create_tag_from_search: Rc<dyn Fn()> = {
            let search_entry = search_entry.clone();
            let available_tags = available_tags.clone();
            let current_tags = current_tags.clone();
            let render_rows_handle = render_rows_handle.clone();
            Rc::new(glib::clone!(
                #[weak(rename_to = window)]
                self,
                #[strong]
                entry_id,
                move || {
                    let tag = search_entry.text().trim().to_string();
                    if tag.is_empty() {
                        return;
                    }

                    let result = {
                        let mut engine = window.imp().engine.borrow_mut();
                        engine.add_tag(handle, &entry_id, &tag)
                    };

                    if let Ok(tags) = result {
                        if !available_tags.borrow().iter().any(|existing| existing == &tag) {
                            available_tags.borrow_mut().push(tag.clone());
                            available_tags.borrow_mut().sort_unstable();
                        }
                        *current_tags.borrow_mut() = tags.clone();
                        *window.imp().current_entry_tags.borrow_mut() = tags;
                        search_entry.set_text("");
                        window.refresh_notes_grid();

                        if let Some(render_rows) = render_rows_handle.borrow().as_ref() {
                            render_rows();
                        }
                    }
                }
            ))
        };

        let render_rows: Rc<dyn Fn()> = {
            let list_box = list_box.clone();
            let search_entry = search_entry.clone();
            let available_tags = available_tags.clone();
            let current_tags = current_tags.clone();
            let entry_id = entry_id.clone();
            Rc::new(glib::clone!(
                #[weak(rename_to = window)]
                self,
                move || {
                    while let Some(child) = list_box.first_child() {
                        list_box.remove(&child);
                    }

                    let filter = search_entry.text().trim().to_lowercase();
                    let mut tags = available_tags.borrow().clone();
                    tags.sort_by_key(|tag| {
                        let selected = current_tags.borrow().iter().any(|current| current == tag);
                        (!selected, tag.to_lowercase())
                    });

                    for tag in tags {
                        if !filter.is_empty() && !tag.to_lowercase().contains(&filter) {
                            continue;
                        }

                        let row = gtk::ListBoxRow::new();
                        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
                        row_box.set_margin_top(8);
                        row_box.set_margin_bottom(8);
                        row_box.set_margin_start(12);
                        row_box.set_margin_end(12);

                        let label = gtk::Label::new(Some(&tag));
                        label.set_hexpand(true);
                        label.set_halign(gtk::Align::Start);
                        label.set_xalign(0.0);

                        let toggle = gtk::CheckButton::new();
                        toggle.set_active(current_tags.borrow().iter().any(|current| current == &tag));

                        toggle.connect_toggled(glib::clone!(
                            #[weak(rename_to = window)]
                            window,
                            #[strong]
                            current_tags,
                            #[strong]
                            tag,
                            #[strong]
                            entry_id,
                            move |button| {
                                let result = {
                                    let mut engine = window.imp().engine.borrow_mut();
                                    if button.is_active() {
                                        engine.add_tag(handle, &entry_id, &tag)
                                    } else {
                                        engine.remove_tag(handle, &entry_id, &tag)
                                    }
                                };

                                if let Ok(tags) = result {
                                    *current_tags.borrow_mut() = tags.clone();
                                    *window.imp().current_entry_tags.borrow_mut() = tags;
                                    window.refresh_notes_grid();
                                }
                            }
                        ));

                        row_box.append(&label);
                        row_box.append(&toggle);
                        row.set_child(Some(&row_box));
                        list_box.append(&row);
                    }

                    if let Some(first_row) = list_box.row_at_index(0) {
                        list_box.select_row(Some(&first_row));
                    }
                }
            ))
        };

        let move_selected_row: Rc<dyn Fn(i32)> = {
            let list_box = list_box.clone();
            Rc::new(move |delta| {
                let current_index = list_box.selected_row().map(|row| row.index()).unwrap_or(0);
                let next_index = if delta < 0 {
                    current_index.saturating_sub(1)
                } else {
                    current_index.saturating_add(1)
                };

                if let Some(next_row) = list_box.row_at_index(next_index) {
                    list_box.select_row(Some(&next_row));
                    next_row.grab_focus();
                }
            })
        };

        let toggle_selected_row: Rc<dyn Fn() -> bool> = {
            let list_box = list_box.clone();
            Rc::new(move || {
                let Some(row) = list_box.selected_row() else {
                    return false;
                };
                let Some(row_child) = row.child() else {
                    return false;
                };
                let Ok(row_box) = row_child.downcast::<gtk::Box>() else {
                    return false;
                };
                let Some(toggle_widget) = row_box.last_child() else {
                    return false;
                };
                let Ok(toggle) = toggle_widget.downcast::<gtk::CheckButton>() else {
                    return false;
                };

                toggle.set_active(!toggle.is_active());
                true
            })
        };

        let dialog_keys = gtk::EventControllerKey::new();
        dialog_keys.connect_key_pressed(glib::clone!(
            #[weak]
            dialog,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, keyval, _, _| {
                if matches!(keyval, gdk::Key::Escape) {
                    dialog.close();
                    return glib::Propagation::Stop;
                }

                glib::Propagation::Proceed
            }
        ));
        dialog.add_controller(dialog_keys);

        let search_keys = gtk::EventControllerKey::new();
        search_keys.connect_key_pressed(glib::clone!(
            #[strong]
            move_selected_row,
            #[strong]
            toggle_selected_row,
            #[strong]
            create_tag_from_search,
            move |_, keyval, _, _| {
                if matches!(keyval, gdk::Key::Up | gdk::Key::KP_Up) {
                    move_selected_row(-1);
                    return glib::Propagation::Stop;
                }

                if matches!(keyval, gdk::Key::Down | gdk::Key::KP_Down) {
                    move_selected_row(1);
                    return glib::Propagation::Stop;
                }

                if matches!(keyval, gdk::Key::space | gdk::Key::KP_Space) {
                    if toggle_selected_row() {
                        return glib::Propagation::Stop;
                    }
                }

                if matches!(keyval, gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::ISO_Enter) {
                    if toggle_selected_row() {
                        return glib::Propagation::Stop;
                    }

                    create_tag_from_search();
                    return glib::Propagation::Stop;
                }

                glib::Propagation::Proceed
            }
        ));
        search_entry.add_controller(search_keys);

        let list_keys = gtk::EventControllerKey::new();
        list_keys.connect_key_pressed(glib::clone!(
            #[strong]
            toggle_selected_row,
            move |_, keyval, _, _| {
                if matches!(keyval, gdk::Key::space | gdk::Key::KP_Space)
                    || matches!(keyval, gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::ISO_Enter)
                {
                    if toggle_selected_row() {
                        return glib::Propagation::Stop;
                    }
                }

                glib::Propagation::Proceed
            }
        ));
        list_box.add_controller(list_keys);

        search_entry.connect_search_changed(glib::clone!(
            #[strong]
            render_rows,
            move |_| {
                render_rows();
            }
        ));

        search_entry.connect_activate(glib::clone!(
            #[strong]
            create_tag_from_search,
            move |_| {
                create_tag_from_search();
            }
        ));

        *render_rows_handle.borrow_mut() = Some(render_rows.clone());

        render_rows();
        dialog.present();
        search_entry.grab_focus();
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
                Self::apply_tag_by_offset(&buffer, TAG_LIST_MARKER, line_start_offset, line_start_offset + marker_len);
                Self::apply_tag_by_offset(&buffer, TAG_LIST_ITEM, line_start_offset + marker_len, line_end_offset);
            } else if let Some(marker_len) = Self::parse_unordered_list_item(line_trimmed) {
                Self::apply_tag_by_offset(&buffer, TAG_LIST_MARKER, line_start_offset, line_start_offset + marker_len);
                Self::apply_tag_by_offset(&buffer, TAG_LIST_ITEM, line_start_offset + marker_len, line_end_offset);
            } else if let Some(marker_len) = Self::parse_ordered_list_item(line_trimmed) {
                Self::apply_tag_by_offset(&buffer, TAG_LIST_ITEM, line_start_offset, line_end_offset);
                Self::apply_tag_by_offset(&buffer, TAG_LIST_MARKER, line_start_offset, line_start_offset + marker_len);
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
        ["• ", "- ", "* ", "+ "]
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

    fn expand_code_block_from_backticks(&self) -> bool {
        let imp = self.imp();
        if *imp.editor_viewer_mode.borrow() {
            return false;
        }

        let buffer = imp.editor_view.buffer();
        if buffer.has_selection() {
            return false;
        }

        let insert = buffer.iter_at_mark(&buffer.get_insert());
        let mut line_start = insert;
        line_start.set_line_offset(0);
        let line_start_offset = line_start.offset();
        let insert_offset = insert.offset();

        let line_text = buffer.text(&line_start, &insert, true).to_string();
        if line_text.trim() != "``" {
            return false;
        }

        let mut delete_start = buffer.iter_at_offset(line_start_offset);
        let mut delete_end = buffer.iter_at_offset(insert_offset);
        buffer.delete(&mut delete_start, &mut delete_end);

        let mut insert_iter = buffer.iter_at_offset(line_start_offset);
        buffer.insert(&mut insert_iter, "```\n\n```");

        let selection_start = buffer.iter_at_offset(line_start_offset + 4);
        let selection_end = buffer.iter_at_offset(line_start_offset + 4);
        buffer.select_range(&selection_start, &selection_end);
        true
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
        let mut strike_start = None;
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

            if line[byte_index..].starts_with("~~") {
                if let Some(start) = strike_start.take() {
                    Self::apply_tag_by_offset(buffer, TAG_SYNTAX, line_start_offset + start, line_start_offset + start + 2);
                    Self::apply_tag_by_offset(buffer, TAG_STRIKETHROUGH, line_start_offset + start + 2, line_start_offset + char_index);
                    Self::apply_tag_by_offset(buffer, TAG_SYNTAX, line_start_offset + char_index, line_start_offset + char_index + 2);
                } else {
                    strike_start = Some(char_index);
                }
                index += 2;
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
        *imp.in_notes_grid_view.borrow_mut() = true;
        self.set_editor_only_actions_enabled(false);
        imp.current_entry_tags.borrow_mut().clear();
        self.update_window_title(None);
        imp.content_stack.set_visible_child(&*imp.notes_page);
        imp.app_header_bar.set_visible(true);
        imp.header_revealer.set_reveal_child(true);
        imp.main_page.set_spacing(8);
        imp.main_page.set_margin_top(MAIN_PAGE_MARGIN_NORMAL);
        imp.main_page.set_margin_bottom(MAIN_PAGE_MARGIN_NORMAL);
        imp.main_page.set_margin_start(MAIN_PAGE_MARGIN_NORMAL);
        imp.main_page.set_margin_end(MAIN_PAGE_MARGIN_NORMAL);
        imp.back_to_grid_button.set_visible(false);
        self.update_notes_search_reveal();
    }

    fn show_setup_page(&self) {
        let imp = self.imp();
        *imp.in_editor_view.borrow_mut() = false;
        *imp.in_notes_grid_view.borrow_mut() = false;
        self.set_editor_only_actions_enabled(false);
        imp.current_entry_tags.borrow_mut().clear();
        self.update_window_title(None);
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
        *imp.in_notes_grid_view.borrow_mut() = false;
        self.set_editor_only_actions_enabled(true);
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
        self.update_notes_search_reveal();
        self.queue_follow_editor_cursor();
    }

    fn update_editor_header_reveal(&self, pointer_y: f64) {
        let imp = self.imp();
        if *imp.in_editor_view.borrow() && !*imp.header_visibility_locked.borrow() {
            imp.header_revealer
                .set_reveal_child(pointer_y <= HEADER_REVEAL_HOVER_Y);
        }
    }
}
