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

use adw::prelude::{AdwDialogExt, NavigationPageExt};
use adw::subclass::prelude::*;
use gtk::pango;
use gtk::prelude::*;
use gtk::{gdk, gio, glib};

use crate::i18n;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::conflict::{
    conflict_block_at_line, conflict_style_spans, unresolved_conflict_count, ConflictBlock,
    ConflictSide, ConflictSpanKind,
};
use crate::engine::{
    EngineMock, EntrySnapshot, EntrySummary, JournalHandle, JournalKind, SyncOutcome,
};
use crate::format;
use crate::gestures;
use crate::settings;

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
const TAG_CONFLICT_CURRENT: &str = "conflict-current";
const TAG_CONFLICT_INCOMING: &str = "conflict-incoming";
const TAG_CONFLICT_MARKER: &str = "conflict-marker";
const ACTION_CONFLICT_ACCEPT_CURRENT: &str = "conflict-accept-current";
const ACTION_CONFLICT_ACCEPT_INCOMING: &str = "conflict-accept-incoming";

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
    #[template(resource = "/io/github/enfff/Diary/window.ui")]
    pub struct PennaFrontendWindow {
        #[template_child]
        pub app_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub setup_page: TemplateChild<gtk::Box>,
        #[template_child]
        pub main_page: TemplateChild<gtk::Box>,
        #[template_child]
        pub content_view: TemplateChild<adw::NavigationView>,
        #[template_child]
        pub notes_page: TemplateChild<adw::NavigationPage>,
        #[template_child]
        pub editor_page: TemplateChild<adw::NavigationPage>,
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
        pub notes_empty_state: TemplateChild<gtk::Box>,
        #[template_child]
        pub notes_empty_button: TemplateChild<gtk::Button>,
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
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,

        pub engine: Arc<Mutex<EngineMock>>,
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
        pub grid_selected_entry_id: RefCell<Option<String>>,
        pub header_visibility_locked: RefCell<bool>,
        pub editor_css_provider: RefCell<Option<gtk::CssProvider>>,
        pub editor_font_size_pt: RefCell<i32>,
        pub last_undo_generation: RefCell<u32>,
        pub editor_viewer_mode: RefCell<bool>,
        pub modified: RefCell<bool>,
    }

    impl Default for PennaFrontendWindow {
        fn default() -> Self {
            Self {
                app_stack: TemplateChild::default(),
                setup_page: TemplateChild::default(),
                main_page: TemplateChild::default(),
                content_view: TemplateChild::default(),
                notes_page: TemplateChild::default(),
                editor_page: TemplateChild::default(),
                connect_button: TemplateChild::default(),
                setup_status_label: TemplateChild::default(),
                sync_status_label: TemplateChild::default(),
                notes_search_revealer: TemplateChild::default(),
                notes_search_entry: TemplateChild::default(),
                notes_flowbox: TemplateChild::default(),
                notes_empty_state: TemplateChild::default(),
                notes_empty_button: TemplateChild::default(),
                editor_view: TemplateChild::default(),
                header_revealer: TemplateChild::default(),
                app_header_bar: TemplateChild::default(),
                viewer_mode_button: TemplateChild::default(),
                main_menu_button: TemplateChild::default(),
                back_to_grid_button: TemplateChild::default(),
                toast_overlay: TemplateChild::default(),
                engine: Arc::new(Mutex::new(EngineMock::default())),
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
                grid_selected_entry_id: RefCell::default(),
                header_visibility_locked: RefCell::default(),
                editor_css_provider: RefCell::default(),
                editor_font_size_pt: RefCell::new(EDITOR_FONT_SIZE_DEFAULT_PT),
                editor_viewer_mode: RefCell::default(),
                last_undo_generation: RefCell::default(),
                modified: RefCell::default(),
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
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow, adw::ApplicationWindow,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget,
                    gtk::Native, gtk::Root, gtk::ShortcutManager,
                    gio::ActionGroup, gio::ActionMap;
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

        // The headerbar shows the formatted entry date only while an entry is
        // open in the editor. On the notes grid the title stays "Diary", so
        // format changes must not touch it.
        if !*self.imp().in_editor_view.borrow() {
            return;
        }

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

        let new_entry = gio::SimpleAction::new("new-entry", None);
        new_entry.connect_activate(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| {
                window.create_new_entry();
            }
        ));
        self.add_action(&new_entry);

        let delete_entry = gio::SimpleAction::new("delete-entry", None);
        delete_entry.connect_activate(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| {
                window.delete_current_entry();
            }
        ));
        self.add_action(&delete_entry);

        let sync_journal = gio::SimpleAction::new("sync-journal", None);
        sync_journal.connect_activate(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| {
                window.run_sync_flow();
            }
        ));
        self.add_action(&sync_journal);

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

        let accept_current_conflict = gio::SimpleAction::new(ACTION_CONFLICT_ACCEPT_CURRENT, None);
        accept_current_conflict.connect_activate(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| {
                window.accept_conflict(ConflictSide::Current);
            }
        ));
        accept_current_conflict.set_enabled(false);
        self.add_action(&accept_current_conflict);

        let accept_incoming_conflict =
            gio::SimpleAction::new(ACTION_CONFLICT_ACCEPT_INCOMING, None);
        accept_incoming_conflict.connect_activate(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| {
                window.accept_conflict(ConflictSide::Incoming);
            }
        ));
        accept_incoming_conflict.set_enabled(false);
        self.add_action(&accept_incoming_conflict);

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

    /// Folder-picker entry point for changing the journal repository:
    /// opens a native folder dialog and connects straight to the pick.
    pub fn pick_repository_and_connect(&self) {
        self.choose_repo_folder();
    }

    fn setup_callbacks(&self) {
        let imp = self.imp();

        self.setup_editor_css();
        self.setup_editor_tags();

        imp.connect_button.connect_clicked(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                window.choose_repo_folder();
            }
        ));


        imp.notes_empty_button.connect_clicked(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                window.create_new_entry();
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
                window.update_conflict_actions();
                window.set_entry_modified(true);
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

                window.update_conflict_actions();
                window.queue_follow_editor_cursor();
            }
        ));

        // Right-clicking anywhere in the note offers conflict resolution
        // next to the usual editing entries; the actions enable only while
        // the caret sits inside a well-formed block.
        let conflict_menu = gio::Menu::new();
        conflict_menu.append(Some(&i18n::accept_current_label()), Some("win.conflict-accept-current"));
        conflict_menu.append(
            Some(&i18n::accept_incoming_label()),
            Some("win.conflict-accept-incoming"),
        );
        imp.editor_view.set_extra_menu(Some(&conflict_menu));

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

        // The edge-swipe gesture pops pages inside NavigationView itself,
        // bypassing show_grid_view(). Resync app state (view flags, header,
        // title, focus) whenever a page is popped by any means.
        imp.content_view.connect_popped(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| {
                window.show_grid_view();
            }
        ));

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
                if matches!(keyval.to_unicode(), Some(' '))
                    && window.convert_leading_hyphen_to_bullet()
                {
                    return glib::Propagation::Stop;
                }

                if matches!(keyval.to_unicode(), Some('`'))
                    && window.expand_code_block_from_backticks()
                {
                    return glib::Propagation::Stop;
                }

                if matches!(
                    keyval,
                    gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::ISO_Enter
                ) && window.continue_list_on_enter()
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

                if matches!(
                    keyval,
                    gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::ISO_Enter
                ) {
                    if let Some(button) = window.selected_note_button() {
                        let entry_id = button.widget_name().to_string();
                        if !entry_id.is_empty() {
                            window.open_entry(&entry_id);
                            return glib::Propagation::Stop;
                        }
                    }
                    return glib::Propagation::Proceed;
                }

                if matches!(keyval, gdk::Key::Delete | gdk::Key::KP_Delete) {
                    if window.selected_note_button().is_some() {
                        window.delete_current_entry();
                        return glib::Propagation::Stop;
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

        gestures::install_editor_back_swipe(self);
        gestures::install_editor_back_swipe_touchpad(self);
        self.load_editor_preferences();
        self.show_grid_view();
        self.initialize_repository_state();
    }


    fn load_editor_preferences(&self) {
        let viewer_mode = settings::get_bool(settings::SETTINGS_EDITOR_VIEWER_MODE_KEY);
        *self.imp().editor_viewer_mode.borrow_mut() = viewer_mode;
        self.apply_editor_mode();
    }

    fn toggle_viewer_mode(&self) {
        let imp = self.imp();
        let next = !*imp.editor_viewer_mode.borrow();
        *imp.editor_viewer_mode.borrow_mut() = next;

        let _ = settings::set_bool(settings::SETTINGS_EDITOR_VIEWER_MODE_KEY, next);

        self.apply_editor_mode();
    }

    fn apply_editor_mode(&self) {
        let imp = self.imp();
        let viewer_mode = *imp.editor_viewer_mode.borrow();

        imp.editor_view.set_editable(!viewer_mode);
        imp.editor_view.set_cursor_visible(!viewer_mode);
        imp.editor_view.set_can_focus(true);
        imp.editor_view.set_can_target(!viewer_mode);
        imp.editor_view
            .set_cursor_from_name(if viewer_mode { Some("default") } else { None });
        imp.viewer_mode_button.set_icon_name(if viewer_mode {
            "view-conceal-symbolic"
        } else {
            "view-reveal-symbolic"
        });
        imp.viewer_mode_button
            .set_tooltip_text(Some(if viewer_mode {
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

        let cursor_offset =
            start_offset + prefix_char_count + selected_char_count + suffix_char_count;
        let cursor_iter = buffer.iter_at_offset(cursor_offset);
        buffer.place_cursor(&cursor_iter);

        let mut scroll_iter = cursor_iter;
        imp.editor_view
            .scroll_to_iter(&mut scroll_iter, 0.1, false, 0.0, 0.0);
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
        let (start_offset, end_offset, has_selection) =
            if let Some((mut start, mut end)) = buffer.selection_bounds() {
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
        let (start_offset, end_offset, selected_text) =
            if let Some((mut start, mut end)) = buffer.selection_bounds() {
                if start.offset() > end.offset() {
                    std::mem::swap(&mut start, &mut end);
                }
                (
                    start.offset(),
                    end.offset(),
                    buffer.text(&start, &end, true).to_string(),
                )
            } else {
                let insert = buffer.iter_at_mark(&buffer.get_insert());
                let mut line_start = insert;
                line_start.set_line_offset(0);
                let mut line_end = line_start;
                line_end.forward_to_line_end();
                (
                    line_start.offset(),
                    line_end.offset(),
                    buffer.text(&line_start, &line_end, true).to_string(),
                )
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
        let font_preset = settings::get_str(settings::SETTINGS_EDITOR_FONT_PRESET_KEY);
        let custom_font = settings::get_str(settings::SETTINGS_EDITOR_FONT_CUSTOM_KEY);

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
                    border-radius: 10px;\
                }}\
                /* Hover/press/selection shading is drawn entirely by the\
                 * button as one layered rectangle; the surrounding\
                 * flowboxchild would otherwise stack its own tint (see\
                 * libadwaita _views.scss) and double up the highlight. */\
                flowbox.notes-grid > flowboxchild:hover,\
                flowbox.notes-grid > flowboxchild:active {{\
                    background: none;\
                }}\
                .note-row:hover {{\
                    background-color: alpha(currentColor, 0.07);\
                }}\
                .note-row:active {{\
                    background-color: alpha(currentColor, 0.12);\
                }}\
                flowbox.notes-grid > flowboxchild.note-current {{\
                    border-radius: 10px;\
                    background-color: alpha(currentColor, 0.06);\
                }}\
                flowbox.notes-grid > flowboxchild.note-current:hover {{\
                    background-color: alpha(currentColor, 0.10);\
                }}\
                flowbox.notes-grid > flowboxchild.note-current:active {{\
                    background-color: alpha(currentColor, 0.13);\
                }}\
                .note-row:focus, .note-row:focus-visible, .note-row:focus:focus-visible {{\
                    outline: none;\
                }}\
                flowbox.notes-grid > flowboxchild:focus, \
                flowbox.notes-grid > flowboxchild:focus-visible, \
                flowbox.notes-grid > flowboxchild:focus:focus-visible {{\
                    outline: none;\
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
            if table
                .lookup(tag.name().as_deref().unwrap_or_default())
                .is_none()
            {
                table.add(tag);
            }
        };

        add_tag(
            &gtk::TextTag::builder()
                .name(TAG_HEADING_1)
                .weight(700)
                .scale(1.8)
                .line_height(1.8)
                .build(),
        );
        add_tag(
            &gtk::TextTag::builder()
                .name(TAG_HEADING_2)
                .weight(700)
                .scale(1.5)
                .line_height(1.6)
                .build(),
        );
        add_tag(
            &gtk::TextTag::builder()
                .name(TAG_HEADING_3)
                .weight(700)
                .scale(1.25)
                .line_height(1.4)
                .build(),
        );
        add_tag(
            &gtk::TextTag::builder()
                .name(TAG_HEADING_4)
                .weight(700)
                .scale(1.1)
                .line_height(1.4)
                .build(),
        );
        add_tag(
            &gtk::TextTag::builder()
                .name(TAG_BLOCKQUOTE)
                .style(pango::Style::Italic)
                .left_margin(18)
                .line_height(1.2)
                .build(),
        );
        add_tag(
            &gtk::TextTag::builder()
                .name(TAG_CODE)
                .family("monospace")
                .build(),
        );
        add_tag(
            &gtk::TextTag::builder()
                .name(TAG_CODE_BLOCK)
                .family("monospace")
                .left_margin(18)
                .right_margin(18)
                .line_height(1.2)
                .build(),
        );
        add_tag(&gtk::TextTag::builder().name(TAG_BOLD).weight(700).build());
        add_tag(
            &gtk::TextTag::builder()
                .name(TAG_ITALIC)
                .style(pango::Style::Italic)
                .build(),
        );
        add_tag(
            &gtk::TextTag::builder()
                .name(TAG_STRIKETHROUGH)
                .strikethrough(true)
                .build(),
        );
        add_tag(
            &gtk::TextTag::builder()
                .name(TAG_SYNTAX)
                .foreground_rgba(&gdk::RGBA::new(0.0, 0.0, 0.0, 0.0))
                .scale(0.01)
                .build(),
        );
        add_tag(
            &gtk::TextTag::builder()
                .name(TAG_LIST_MARKER)
                .foreground_rgba(&gdk::RGBA::new(0.45, 0.45, 0.45, 1.0))
                .weight(500)
                .build(),
        );
        add_tag(
            &gtk::TextTag::builder()
                .name(TAG_LIST_ITEM)
                .left_margin(18)
                .build(),
        );
        add_tag(
            &gtk::TextTag::builder()
                .name(TAG_LINK)
                .underline(pango::Underline::Single)
                .build(),
        );
        add_tag(
            &gtk::TextTag::builder()
                .name(TAG_CHECKED)
                .strikethrough(true)
                .build(),
        );
        add_tag(
            &gtk::TextTag::builder()
                .name(TAG_RULE)
                .scale(0.85)
                .weight(700)
                .justification(gtk::Justification::Center)
                .build(),
        );
        add_tag(
            &gtk::TextTag::builder()
                .name(TAG_CONFLICT_CURRENT)
                .background_rgba(&gdk::RGBA::new(0.30, 0.69, 0.31, 0.16))
                .background_full_height(true)
                .build(),
        );
        add_tag(
            &gtk::TextTag::builder()
                .name(TAG_CONFLICT_INCOMING)
                .background_rgba(&gdk::RGBA::new(0.16, 0.50, 0.85, 0.16))
                .background_full_height(true)
                .build(),
        );
        add_tag(
            &gtk::TextTag::builder()
                .name(TAG_CONFLICT_MARKER)
                .foreground_rgba(&gdk::RGBA::new(0.45, 0.45, 0.45, 1.0))
                .style(pango::Style::Italic)
                .build(),
        );
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
        let repo_path = settings::get_str(settings::SETTINGS_REPOSITORY_PATH_KEY);

        if repo_path.trim().is_empty() {
            self.show_setup_page();
            return;
        }

        self.connect_journal(&repo_path);
    }

    fn connect_journal(&self, repo_path: &str) {
        let imp = self.imp();
        self.stop_entries_monitor();
        let repo_path = repo_path.trim().to_string();
        if repo_path.is_empty() {
            imp.setup_status_label.set_label(&i18n::repo_path_required());
            return;
        }

        let connect_result = {
            let mut engine = imp.engine.lock().unwrap();
            engine.connect_journal(&repo_path)
        };

        match connect_result {
            Ok(result) => {
                *imp.current_handle.borrow_mut() = Some(result.journal_handle);

                let _ =
                    settings::set_str(settings::SETTINGS_REPOSITORY_PATH_KEY, &repo_path);

                let sync_message = match result.journal_kind {
                    JournalKind::New => "New diary initialized and connected",
                    JournalKind::Existing => "Existing journal connected",
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

                // Product flow: an already-connected session immediately
                // runs the update flow; a fresh diary has nothing to sync.
                if result.journal_kind == JournalKind::Existing {
                    self.run_sync_flow();
                }
            }
            Err(err) => {
                imp.setup_status_label.set_label(&err);
                imp.sync_status_label.set_label(&err);
            }
        }
    }

    /// Update flow for an already-connected session: fetch/push/merge via the
    /// engine's `sync_journal`, then surface whatever the merge produced.
    fn run_sync_flow(&self) {
        let imp = self.imp();
        let Some(handle) = *imp.current_handle.borrow() else {
            return;
        };

        let outcome = {
            let engine = imp.engine.lock().unwrap();
            engine.sync_journal(handle)
        };

        match outcome {
            Ok(outcome) => self.handle_sync_outcome(&outcome),
            Err(err) => {
                imp.sync_status_label
                    .set_label(&i18n::sync_failed(&err));
                let toast = adw::Toast::new(&i18n::sync_failed(&err));
                toast.set_priority(adw::ToastPriority::High);
                imp.toast_overlay.add_toast(toast);
            }
        }
    }

    fn handle_sync_outcome(&self, outcome: &SyncOutcome) {
        let imp = self.imp();
        self.refresh_notes_grid();

        let message = sync_status_message(outcome);
        imp.sync_status_label.set_label(&message);

        if !outcome.conflicted_entry_ids.is_empty() {
            let count = outcome.conflicted_entry_ids.len();
            let toast = adw::Toast::new(&sync_conflict_toast_message(count));
            toast.set_priority(adw::ToastPriority::High);
            imp.toast_overlay.add_toast(toast);
        }
    }

    /// ADR 0014 conclude step: saving the last resolved note stages it and
    /// clears its index conflict stages; when nothing conflicts anymore, one
    /// follow-up sync creates the merge commit automatically.
    fn maybe_conclude_merge(&self) {
        let imp = self.imp();
        let Some(handle) = *imp.current_handle.borrow() else {
            return;
        };

        let merge_pending = imp
            .engine
            .lock()
            .unwrap()
            .journal_status(handle)
            .is_some_and(|status| status.merge_in_progress);

        if !merge_pending {
            return;
        }

        let outcome = imp.engine.lock().unwrap().sync_journal(handle);
        match outcome {
            Ok(outcome) if outcome.conflicted_entry_ids.is_empty() => {
                self.handle_sync_outcome(&outcome);
                let toast = adw::Toast::new(&i18n::all_conflicts_resolved());
                toast.set_priority(adw::ToastPriority::High);
                imp.toast_overlay.add_toast(toast);
            }
            Ok(_) => {}
            Err(err) => imp
                .sync_status_label
                .set_label(&i18n::sync_failed(&err)),
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
            let engine = imp.engine.lock().unwrap();
            engine.list_entries(handle)
        };

        // Notes still conflicted mid-merge get a warning badge so unresolved
        // sync state is visible without opening each note.
        let conflicted_ids = {
            let engine = imp.engine.lock().unwrap();
            engine.conflicted_entry_ids(handle)
        };

        let mut first_visible_button: Option<gtk::Button> = None;

        for entry in &entries {
            let content = {
                let engine = imp.engine.lock().unwrap();
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

            let note_label =
                gtk::Label::new(Some(&format::format_entry_date(&entry.entry_id)));
            note_label.set_hexpand(true);
            note_label.set_halign(gtk::Align::Start);
            note_label.set_xalign(0.0);
            note_label.set_ellipsize(pango::EllipsizeMode::End);

            let tags_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            tags_box.add_css_class("note-tags");
            tags_box.set_halign(gtk::Align::End);
            tags_box.set_valign(gtk::Align::Center);

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
            if conflicted_ids.iter().any(|id| id == &entry.entry_id) {
                let conflict_icon = gtk::Image::from_icon_name("dialog-warning-symbolic");
                conflict_icon.set_tooltip_text(Some(&i18n::unresolved_sync_conflict()));
                conflict_icon.add_css_class("warning");
                conflict_icon.set_margin_end(8);
                row_box.set_center_widget(Some(&conflict_icon));
            }
            button.set_child(Some(&row_box));

            let entry_id = entry.entry_id.clone();
            button.connect_clicked(glib::clone!(
                #[weak(rename_to = window)]
                self,
                #[strong]
                entry_id,
                move |_| {
                    window.select_note(Some(&entry_id));
                    window.open_entry(entry_id.as_str());
                }
            ));
            if first_visible_button.is_none() {
                first_visible_button = Some(button.clone());
            }
            imp.notes_flowbox.insert(&button, -1);
        }

        // Show the inviting empty state only when the journal has no notes at
        // all. A search that matches nothing leaves the grid empty but does
        // not show the "create your first note" prompt.
        imp.notes_empty_state.set_visible(entries.is_empty());

        // Keep highlighting a stable selection across refreshes; if the
        // previously selected note is gone (deleted, filtered out), fall back
        // to the first visible row so something is always selected.
        let buttons = self.note_buttons();
        let selected_still_visible = imp
            .grid_selected_entry_id
            .borrow()
            .as_deref()
            .is_some_and(|id| buttons.iter().any(|button| button.widget_name() == id));
        if !selected_still_visible {
            *imp.grid_selected_entry_id.borrow_mut() =
                buttons.first().map(|b| b.widget_name().to_string());
        }
        self.refresh_grid_selection();

        if query.is_empty() && *imp.in_notes_grid_view.borrow() {
            if let Some(button) = first_visible_button {
                button.grab_focus();
            }
        }

        if let Some(status) = imp.engine.lock().unwrap().journal_status(handle) {
            let mut details = format!(
                "Branch: {} | head: {} | dirty: {} | entries: {}",
                status.branch, status.head_commit, status.dirty, status.entry_count
            );
            if status.merge_in_progress {
                details.push_str(&format!(
                    " | merge in progress: {} unresolved",
                    status.conflicted_entry_ids.len()
                ));
            }
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

    fn selected_note_button(&self) -> Option<gtk::Button> {
        let selected = self.imp().grid_selected_entry_id.borrow().clone()?;
        self.note_buttons()
            .into_iter()
            .find(|button| button.widget_name() == selected.as_str())
    }

    /// Marks `entry_id` as the grid's current selection and paints the
    /// persistent highlight. Selection is independent of GTK keyboard-focus
    /// visibility, so it is visible before any arrow key is pressed.
    fn select_note(&self, entry_id: Option<&str>) {
        *self.imp().grid_selected_entry_id.borrow_mut() = entry_id.map(str::to_string);
        self.refresh_grid_selection();
    }

    fn refresh_grid_selection(&self) {
        let selected = self.imp().grid_selected_entry_id.borrow().clone();
        for button in self.note_buttons() {
            let is_selected = selected.as_deref() == Some(button.widget_name().as_str());
            // Paint the selection on the flowboxchild wrapper, not the
            // button: libadwaita draws hover/active feedback on that same
            // wrapper, so keeping one painted layer avoids stacked tints.
            if let Some(wrapper) = button
                .parent()
                .and_then(|widget| widget.downcast::<gtk::FlowBoxChild>().ok())
            {
                if is_selected {
                    wrapper.add_css_class("note-current");
                } else {
                    wrapper.remove_css_class("note-current");
                }
            }
        }
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

        let selected_id = self.imp().grid_selected_entry_id.borrow().clone();
        let current = buttons
            .iter()
            .position(|button| selected_id.as_deref() == Some(button.widget_name().as_str()))
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
                self.select_note(Some(&button.widget_name()));
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
        imp.notes_search_entry
            .set_position(text.chars().count() as i32);
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
            ),
        );

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
                ),
            );

            *imp.cursor_follow_late_source.borrow_mut() = Some(late_source);
        }
    }

    fn open_entry(&self, entry_id: &str) {
        let imp = self.imp();
        let Some(handle) = *imp.current_handle.borrow() else {
            return;
        };

        let content = {
            let engine = imp.engine.lock().unwrap();
            engine.get_entry(handle, entry_id)
        };

        let Some(entry) = content else {
            return;
        };

        *imp.current_entry_id.borrow_mut() = Some(entry_id.to_string());
        *imp.current_entry_tags.borrow_mut() = entry.tags.clone();
        self.update_window_title(Some(entry_id));
        imp.editor_view.buffer().set_text(&entry.content);
        // Loading content fires the change handler above; the freshly loaded
        // note is by definition saved.
        self.set_entry_modified(false);
        self.apply_editor_mode();
        self.apply_markdown_styling();
        self.show_editor_view();
    }

    fn save_current_entry(&self) {
        let imp = self.imp();
        let Some(handle) = *imp.current_handle.borrow() else {
            imp.sync_status_label
                .set_label(&i18n::connect_repository_before_saving());
            return;
        };

        let Some(entry_id) = imp.current_entry_id.borrow().clone() else {
            imp.sync_status_label.set_label(&i18n::no_entry_selected());
            return;
        };

        let buffer = imp.editor_view.buffer();
        let (start, end) = buffer.bounds();
        let content = buffer.text(&start, &end, true).to_string();

        // The editor reapplies the conflict tags from the same parser on
        // every change, so counting parsed blocks is equivalent to scanning
        // those tags — and refuses to save while any remain.
        let unresolved = unresolved_conflict_count(&content);
        if unresolved > 0 {
            let toast = adw::Toast::new(&i18n::unresolved_conflicts(unresolved));
            toast.set_priority(adw::ToastPriority::High);
            imp.toast_overlay.add_toast(toast);
            return;
        }

        let tags = imp.current_entry_tags.borrow().clone();

        let save_result = {
            let mut engine = imp.engine.lock().unwrap();
            engine.entry_save(handle, &entry_id, &content, &tags)
        };

        match save_result {
            Ok(()) => {
                self.set_entry_modified(false);
                imp.sync_status_label.set_label(&i18n::saved());
                self.refresh_notes_grid();
                self.show_editor_view();
                self.maybe_conclude_merge();
            }
            Err(err) => {
                imp.sync_status_label.set_label(&err);
            }
        }
    }

    fn create_new_entry(&self) {
        let imp = self.imp();
        let Some(handle) = *imp.current_handle.borrow() else {
            return;
        };

        // One note per day: if today already has an entry, redirect to it
        // instead of creating a duplicate.
        let today = chrono::Local::now().format("%Y%m%d").to_string();
        let existing = {
            let engine = imp.engine.lock().unwrap();
            Self::entry_id_for_day(&engine.list_entries(handle), &today)
        };

        if let Some(existing) = existing {
            let toast = adw::Toast::new(&i18n::opened_todays_note());
            imp.toast_overlay.add_toast(toast);
            self.open_entry(&existing);
            return;
        }

        let record = {
            let mut engine = imp.engine.lock().unwrap();
            match engine.create_entry_new(handle) {
                Ok(record) => record,
                Err(err) => {
                    imp.sync_status_label.set_label(&err);
                    return;
                }
            }
        };

        *imp.current_entry_id.borrow_mut() = Some(record.entry_id.clone());
        *imp.current_entry_tags.borrow_mut() = record.tags.clone();
        self.update_window_title(Some(&record.entry_id));
        imp.editor_view.buffer().set_text(&record.content);
        self.set_entry_modified(false);
        self.apply_editor_mode();
        self.apply_markdown_styling();
        self.show_editor_view();
        self.refresh_notes_grid();
    }

    fn delete_current_entry(&self) {
        let imp = self.imp();
        let Some(handle) = *imp.current_handle.borrow() else {
            imp.sync_status_label
                .set_label(&i18n::connect_repository_before_deleting());
            return;
        };
        let entry_id = if *imp.in_notes_grid_view.borrow() {
            self.selected_note_button()
                .map(|button| button.widget_name().to_string())
                .or_else(|| imp.current_entry_id.borrow().clone())
        } else {
            imp.current_entry_id.borrow().clone()
        };

        let Some(entry_id) = entry_id else {
            imp.sync_status_label.set_label(&i18n::no_note_selected());
            return;
        };

        let snapshot = {
            let mut engine = imp.engine.lock().unwrap();
            match engine.delete_entry_with_snapshot(handle, &entry_id) {
                Ok(snapshot) => snapshot,
                Err(err) => {
                    imp.sync_status_label.set_label(&err);
                    return;
                }
            }
        };

        *imp.current_entry_id.borrow_mut() = None;
        imp.current_entry_tags.borrow_mut().clear();
        self.set_entry_modified(false);
        self.show_grid_view();
        self.refresh_notes_grid();
        self.show_delete_undo_toast(snapshot);
    }

    fn show_delete_undo_toast(&self, snapshot: EntrySnapshot) {
        let imp = self.imp();
        if imp.current_handle.borrow().is_none() {
            return;
        }

        let generation = *imp.last_undo_generation.borrow() + 1;
        *imp.last_undo_generation.borrow_mut() = generation;

        let toast = adw::Toast::new(&i18n::note_deleted());
        toast.set_button_label(Some(&i18n::undo_button_label()));
        toast.set_timeout(5);
        toast.set_priority(adw::ToastPriority::High);

        toast.connect_button_clicked(glib::clone!(
            #[weak(rename_to = window)]
            self,
            #[strong]
            snapshot,
            #[strong]
            generation,
            move |toast| {
                let window_imp = window.imp();
                if *window_imp.last_undo_generation.borrow() != generation {
                    // A newer delete happened; this undo is stale.
                    toast.dismiss();
                    return;
                }

                let Some(handle) = *window_imp.current_handle.borrow() else {
                    toast.dismiss();
                    return;
                };

                let result = {
                    let mut engine = window_imp.engine.lock().unwrap();
                    engine.restore_entry(handle, &snapshot)
                };

                match result {
                    Ok(record) => {
                        window_imp.sync_status_label.set_label(&i18n::note_restored());
                        window.refresh_notes_grid();
                        window.open_entry(&record.entry_id);
                    }
                    Err(err) => {
                        window_imp.sync_status_label.set_label(&err);
                    }
                }
                toast.dismiss();
            }
        ));

        imp.toast_overlay.add_toast(toast);
    }

    fn choose_repo_folder(&self) {
        let dialog = gtk::FileDialog::builder()
            .title(i18n::choose_repository_folder().as_str())
            .accept_label("Select")
            .modal(true)
            .build();

        dialog.select_folder(
            Some(self),
            None::<&gio::Cancellable>,
            glib::clone!(
                #[weak(rename_to = window)]
                self,
                move |result| {
                    if let Ok(file) = result {
                        if let Some(path) = file.path() {
                            let path_str = path.to_string_lossy().to_string();
                            window.connect_journal(&path_str);
                        }
                    }
                }
            ),
        );
    }

    fn start_repo_watchers(&self) {
        self.stop_repo_watchers();

        let imp = self.imp();
        let Some(handle) = *imp.current_handle.borrow() else {
            return;
        };

        let initial_fingerprint = {
            let engine = imp.engine.lock().unwrap();
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
            let engine = imp.engine.lock().unwrap();
            engine.entries_directory(handle)
        };

        let Some(watch_path) = watch_path else {
            return;
        };

        let file = gio::File::for_path(watch_path);
        let monitor =
            match file.monitor_directory(gio::FileMonitorFlags::NONE, None::<&gio::Cancellable>) {
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
            let engine = imp.engine.lock().unwrap();
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
            let mut engine = imp.engine.lock().unwrap();
            engine.reload_entries(handle)
        };

        match reload_result {
            Ok(count) => {
                self.refresh_notes_grid();

                let new_fingerprint = {
                    let engine = imp.engine.lock().unwrap();
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

    /// Entry IDs start with an 8-digit `YYYYMMDD` day prefix. Returns the
    /// latest entry (highest id) belonging to that day, if any.
    fn entry_id_for_day(entries: &[EntrySummary], day_prefix: &str) -> Option<String> {
        entries
            .iter()
            .filter(|entry| entry.entry_id.starts_with(day_prefix))
            .map(|entry| entry.entry_id.clone())
            .max()
    }

    fn update_window_title(&self, entry_id: Option<&str>) {
        let imp = self.imp();
        let base = entry_id
            .map(format::format_entry_date)
            .filter(|text| !text.is_empty())
            .map(|text| text.to_string())
            .unwrap_or_else(i18n::diary_title);

        // GNOME HIG dirty-document indicator: a bullet before the title while
        // the open note has unsaved edits.
        let title = if *imp.modified.borrow() {
            format!("• {base}")
        } else {
            base
        };

        self.set_title(Some(&title));
    }

    fn set_entry_modified(&self, modified: bool) {
        let imp = self.imp();
        if *imp.modified.borrow() == modified {
            return;
        }
        *imp.modified.borrow_mut() = modified;
        self.update_window_title(imp.current_entry_id.borrow().as_deref());
    }

    fn visible_tags_for_row(tags: &[String]) -> Vec<&str> {
        let mut visible = Vec::new();
        let mut used_chars = 0usize;

        for tag in tags {
            let tag_chars = tag.chars().count();
            let next_cost = if visible.is_empty() {
                tag_chars
            } else {
                tag_chars + 1
            };

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
        tags.len()
            .saturating_sub(Self::visible_tags_for_row(tags).len())
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

        let dialog = adw::Dialog::new();
        dialog.set_title("Tags");
        dialog.set_content_width(400);
        dialog.set_content_height(500);

        const ADD_ROW_NAME: &str = "+add-tag";

        let toolbar = adw::ToolbarView::new();
        let header = adw::HeaderBar::new();
        header.add_css_class("flat");
        header.set_title_widget(Some(&adw::WindowTitle::new("Tags", "")));
        toolbar.add_top_bar(&header);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.set_margin_start(12);
        content.set_margin_end(12);
        toolbar.set_content(Some(&content));
        dialog.set_child(Some(&toolbar));

        let search_entry = gtk::SearchEntry::new();
        search_entry.set_placeholder_text(Some("Filter or create a tag"));
        content.append(&search_entry);

        let chips_flow = gtk::FlowBox::new();
        chips_flow.set_selection_mode(gtk::SelectionMode::None);
        chips_flow.set_activate_on_single_click(false);
        chips_flow.set_halign(gtk::Align::Start);
        chips_flow.set_valign(gtk::Align::Start);
        chips_flow.set_max_children_per_line(6);
        chips_flow.set_column_spacing(6);
        chips_flow.set_row_spacing(6);
        content.append(&chips_flow);

        let scrolled = gtk::ScrolledWindow::new();
        scrolled.set_hscrollbar_policy(gtk::PolicyType::Never);
        scrolled.set_vscrollbar_policy(gtk::PolicyType::Automatic);
        scrolled.set_propagate_natural_height(true);
        scrolled.set_max_content_height(320);
        scrolled.set_min_content_height(180);

        let list_box = gtk::ListBox::new();
        list_box.add_css_class("boxed-list");
        list_box.set_selection_mode(gtk::SelectionMode::None);
        scrolled.set_child(Some(&list_box));

        let empty_page = gtk::Box::new(gtk::Orientation::Vertical, 6);
        empty_page.set_valign(gtk::Align::Center);
        empty_page.set_halign(gtk::Align::Center);
        let empty_title = gtk::Label::new(Some("No tags yet"));
        empty_title.add_css_class("title-2");
        let empty_hint = gtk::Label::new(Some("Type a name above to create your first tag."));
        empty_hint.add_css_class("dim-label");
        empty_hint.set_wrap(true);
        empty_page.append(&empty_title);
        empty_page.append(&empty_hint);

        let pages = gtk::Stack::new();
        pages.set_vexpand(true);
        pages.add_named(&scrolled, Some("list"));
        pages.add_named(&empty_page, Some("empty"));
        content.append(&pages);

        let available_tags = Rc::new(RefCell::new({
            let engine = imp.engine.lock().unwrap();
            engine.list_tags(handle)
        }));
        let current_tags = Rc::new(RefCell::new(imp.current_entry_tags.borrow().clone()));
        type RenderRowsHandle = Rc<RefCell<Option<Rc<dyn Fn()>>>>;
        let render_rows_handle: RenderRowsHandle = Rc::new(RefCell::new(None));

        let render_chips: Rc<dyn Fn()> = {
            let chips_flow = chips_flow.clone();
            let current_tags = current_tags.clone();
            Rc::new(move || {
                while let Some(child) = chips_flow.first_child() {
                    chips_flow.remove(&child);
                }
                let mut tags = current_tags.borrow().clone();
                tags.sort_unstable();
                for tag in tags {
                    chips_flow.append(&Self::build_tag_chip(&tag));
                }
            })
        };

        // Single path for every tag mutation: engine call, state sync, grid
        // refresh, chip strip, and check-button echo. Handlers that fire from
        // widgets must guard against echoes before calling this.
        type ApplyTagFn = Rc<dyn Fn(&str, bool)>;
        let apply_tag: ApplyTagFn = {
            let list_box = list_box.clone();
            let current_tags = current_tags.clone();
            let render_chips = render_chips.clone();
            Rc::new(glib::clone!(
                #[weak(rename_to = window)]
                self,
                #[strong]
                entry_id,
                move |tag: &str, attach: bool| {
                    let result = {
                        let mut engine = window.imp().engine.lock().unwrap();
                        if attach {
                            engine.add_tag(handle, &entry_id, tag)
                        } else {
                            engine.remove_tag(handle, &entry_id, tag)
                        }
                    };

                    if let Ok(tags) = result {
                        *current_tags.borrow_mut() = tags.clone();
                        *window.imp().current_entry_tags.borrow_mut() = tags;
                        window.refresh_notes_grid();
                        render_chips();

                        let mut iter = list_box.first_child();
                        while let Some(child) = iter {
                            iter = child.next_sibling();
                            if child.widget_name() != tag {
                                continue;
                            }
                            if let Some(check) = child
                                .downcast_ref::<gtk::ListBoxRow>()
                                .and_then(|row| row.child())
                                .and_then(|c| c.downcast::<gtk::Box>().ok())
                                .and_then(|b| b.last_child())
                                .and_then(|w| w.downcast::<gtk::CheckButton>().ok())
                            {
                                check.set_active(attach);
                            }
                        }
                    }
                }
            ))
        };

        let create_tag_from_search: Rc<dyn Fn()> = {
            let search_entry = search_entry.clone();
            let available_tags = available_tags.clone();
            let current_tags = current_tags.clone();
            let render_rows_handle = render_rows_handle.clone();
            let render_chips = render_chips.clone();
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
                        let mut engine = window.imp().engine.lock().unwrap();
                        engine.add_tag(handle, &entry_id, &tag)
                    };

                    if let Ok(tags) = result {
                        if !available_tags
                            .borrow()
                            .iter()
                            .any(|existing| existing == &tag)
                        {
                            available_tags.borrow_mut().push(tag.clone());
                            available_tags.borrow_mut().sort_unstable();
                        }
                        *current_tags.borrow_mut() = tags.clone();
                        *window.imp().current_entry_tags.borrow_mut() = tags;
                        search_entry.set_text("");
                        window.refresh_notes_grid();
                        render_chips();

                        if let Some(render_rows) = render_rows_handle.borrow().as_ref() {
                            render_rows();
                        }
                    }
                }
            ))
        };

        // One activation path shared by click, Enter-on-row, and Space.
        let activate_row: Rc<dyn Fn(&gtk::ListBoxRow)> = {
            let current_tags = current_tags.clone();
            let create_tag_from_search = create_tag_from_search.clone();
            let apply_tag = apply_tag.clone();
            Rc::new(move |row| {
                let name = row.widget_name().to_string();
                if name == ADD_ROW_NAME {
                    create_tag_from_search();
                    return;
                }

                let attached = current_tags.borrow().iter().any(|c| c == &name);
                apply_tag(&name, !attached);
            })
        };

        let render_rows: Rc<dyn Fn()> = {
            let list_box = list_box.clone();
            let pages = pages.clone();
            let search_entry = search_entry.clone();
            let available_tags = available_tags.clone();
            let current_tags = current_tags.clone();
            let apply_tag = apply_tag.clone();
            Rc::new(move || {
                // Remember the focused row across the rebuild so keyboard
                // navigation is not thrown back to the top on every filter
                // keystroke.
                let mut focused_tag: Option<String> = None;
                let mut iter = list_box.first_child();
                while let Some(child) = iter {
                    iter = child.next_sibling();
                    if let Some(row) = child.downcast_ref::<gtk::ListBoxRow>() {
                        if row.has_focus() {
                            focused_tag = Some(row.widget_name().to_string());
                        }
                    }
                }

                while let Some(child) = list_box.first_child() {
                    list_box.remove(&child);
                }

                let query = search_entry.text().trim().to_string();
                let lower_query = query.to_lowercase();

                let mut tags = available_tags.borrow().clone();
                tags.sort_unstable();

                if !query.is_empty() && !tags.iter().any(|t| t.to_lowercase() == lower_query) {
                    let row = gtk::ListBoxRow::new();
                    row.set_widget_name(ADD_ROW_NAME);
                    let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                    row_box.set_margin_top(8);
                    row_box.set_margin_bottom(8);
                    row_box.set_margin_start(12);
                    row_box.set_margin_end(12);

                    let icon = gtk::Image::from_icon_name("list-add-symbolic");
                    let label = gtk::Label::new(None);
                    label.set_markup(&format!(
                        "Add tag \u{201C}<b>{}</b>\u{201D}",
                        glib::markup_escape_text(&query)
                    ));
                    label.set_halign(gtk::Align::Start);

                    row_box.append(&icon);
                    row_box.append(&label);
                    row.set_child(Some(&row_box));
                    list_box.append(&row);
                }

                for tag in &tags {
                    if !lower_query.is_empty() && !tag.to_lowercase().contains(&lower_query) {
                        continue;
                    }

                    let attached = current_tags.borrow().iter().any(|current| current == tag);

                    let row = gtk::ListBoxRow::new();
                    row.set_widget_name(tag.as_str());
                    let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
                    row_box.set_margin_top(8);
                    row_box.set_margin_bottom(8);
                    row_box.set_margin_start(12);
                    row_box.set_margin_end(12);

                    let label = gtk::Label::new(Some(tag));
                    label.set_hexpand(true);
                    label.set_halign(gtk::Align::Start);
                    label.set_xalign(0.0);
                    label.set_ellipsize(pango::EllipsizeMode::End);

                    let toggle = gtk::CheckButton::new();
                    toggle.set_active(attached);
                    toggle.connect_toggled(glib::clone!(
                        #[strong]
                        current_tags,
                        #[strong]
                        apply_tag,
                        #[strong]
                        tag,
                        move |button| {
                            let attached_now = current_tags.borrow().contains(&tag);
                            if attached_now == button.is_active() {
                                return; // programmatic echo, not a user action
                            }
                            apply_tag(&tag, button.is_active());
                        }
                    ));

                    row_box.append(&label);
                    row_box.append(&toggle);
                    row.set_child(Some(&row_box));
                    list_box.append(&row);
                }

                let no_tags_at_all = tags.is_empty();
                pages.set_visible_child_name(if no_tags_at_all && query.is_empty() {
                    "empty"
                } else {
                    "list"
                });

                if let Some(name) = focused_tag {
                    let mut iter = list_box.first_child();
                    while let Some(child) = iter {
                        iter = child.next_sibling();
                        if child.widget_name() == name {
                            if let Some(row) = child.downcast_ref::<gtk::ListBoxRow>() {
                                row.grab_focus();
                            }
                            break;
                        }
                    }
                }
            })
        };

        // Click and Enter-on-row both arrive here; Space is routed manually
        // below since ListBoxRow only binds Return to its activate signal.
        list_box.connect_row_activated(glib::clone!(
            #[strong]
            activate_row,
            move |_, row| {
                activate_row(row);
            }
        ));

        let search_keys = gtk::EventControllerKey::new();
        search_keys.connect_key_pressed(glib::clone!(
            #[strong]
            list_box,
            #[strong]
            search_entry,
            #[strong]
            available_tags,
            #[strong]
            activate_row,
            #[strong]
            create_tag_from_search,
            move |_, keyval, _, _| {
                if matches!(
                    keyval,
                    gdk::Key::Up | gdk::Key::KP_Up | gdk::Key::Down | gdk::Key::KP_Down
                ) {
                    if list_box.focus_child().is_none() {
                        if let Some(first) = list_box.row_at_index(0) {
                            first.grab_focus();
                        }
                    }
                    return glib::Propagation::Stop;
                }

                if matches!(
                    keyval,
                    gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::ISO_Enter
                ) {
                    let query = search_entry.text().trim().to_string();
                    if query.is_empty() {
                        return glib::Propagation::Proceed;
                    }

                    let exact = available_tags
                        .borrow()
                        .iter()
                        .find(|tag| tag.to_lowercase() == query.to_lowercase())
                        .cloned();

                    match exact {
                        Some(tag) => {
                            let mut iter = list_box.first_child();
                            while let Some(child) = iter {
                                iter = child.next_sibling();
                                if child.widget_name() == tag {
                                    if let Some(row) = child.downcast_ref::<gtk::ListBoxRow>() {
                                        activate_row(row);
                                    }
                                    break;
                                }
                            }
                        }
                        None => create_tag_from_search(),
                    }

                    return glib::Propagation::Stop;
                }

                glib::Propagation::Proceed
            }
        ));
        search_entry.add_controller(search_keys);

        let list_keys = gtk::EventControllerKey::new();
        list_keys.connect_key_pressed(glib::clone!(
            #[strong]
            list_box,
            #[strong]
            activate_row,
            move |_, keyval, _, _| {
                if matches!(keyval, gdk::Key::space | gdk::Key::KP_Space) {
                    if let Some(row) = list_box.focus_child().and_downcast::<gtk::ListBoxRow>() {
                        activate_row(&row);
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

        render_chips();
        render_rows();
        dialog.present(Some(self));
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
                Self::apply_tag_by_offset(
                    &buffer,
                    TAG_CODE_BLOCK,
                    line_start_offset,
                    line_end_offset,
                );
                Self::apply_tag_by_offset(&buffer, TAG_SYNTAX, line_start_offset, line_end_offset);
                line_start_offset = line_end_offset + 1;
                continue;
            }

            if in_code_block {
                Self::apply_tag_by_offset(
                    &buffer,
                    TAG_CODE_BLOCK,
                    line_start_offset,
                    line_end_offset,
                );
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
                Self::apply_tag_by_offset(
                    &buffer,
                    TAG_SYNTAX,
                    line_start_offset,
                    line_start_offset + marker_len,
                );
                Self::apply_tag_by_offset(
                    &buffer,
                    tag,
                    line_start_offset + marker_len,
                    line_end_offset,
                );
            }

            if let Some(content) = line_trimmed.strip_prefix("> ") {
                let prefix_len = line_trimmed.chars().count() - content.chars().count();
                Self::apply_tag_by_offset(
                    &buffer,
                    TAG_SYNTAX,
                    line_start_offset,
                    line_start_offset + prefix_len,
                );
                Self::apply_tag_by_offset(
                    &buffer,
                    TAG_BLOCKQUOTE,
                    line_start_offset + prefix_len,
                    line_end_offset,
                );
            }

            if let Some(marker_len) =
                Self::parse_checkbox_item(line_trimmed).map(|item| item.marker_len)
            {
                Self::apply_tag_by_offset(
                    &buffer,
                    TAG_LIST_MARKER,
                    line_start_offset,
                    line_start_offset + marker_len,
                );
                Self::apply_tag_by_offset(
                    &buffer,
                    TAG_LIST_ITEM,
                    line_start_offset + marker_len,
                    line_end_offset,
                );
            } else if let Some(marker_len) = Self::parse_unordered_list_item(line_trimmed) {
                Self::apply_tag_by_offset(
                    &buffer,
                    TAG_LIST_MARKER,
                    line_start_offset,
                    line_start_offset + marker_len,
                );
                Self::apply_tag_by_offset(
                    &buffer,
                    TAG_LIST_ITEM,
                    line_start_offset + marker_len,
                    line_end_offset,
                );
            } else if let Some(marker_len) = Self::parse_ordered_list_item(line_trimmed) {
                Self::apply_tag_by_offset(
                    &buffer,
                    TAG_LIST_ITEM,
                    line_start_offset,
                    line_end_offset,
                );
                Self::apply_tag_by_offset(
                    &buffer,
                    TAG_LIST_MARKER,
                    line_start_offset,
                    line_start_offset + marker_len,
                );
            }

            if Self::is_horizontal_rule(line_trimmed) {
                Self::apply_tag_by_offset(&buffer, TAG_RULE, line_start_offset, line_end_offset);
            }

            Self::apply_inline_markdown_tags(&buffer, line, line_start_offset);
            line_start_offset = line_end_offset + 1;
        }

        Self::apply_conflict_styling(&buffer, &text);
    }

    fn apply_conflict_styling(buffer: &gtk::TextBuffer, text: &str) {
        let spans = conflict_style_spans(text);
        if spans.is_empty() {
            return;
        }

        let mut line_bounds: Vec<(usize, usize)> = Vec::new();
        let mut position = 0usize;
        for line in text.split_inclusive('\n') {
            let width = line.chars().count();
            line_bounds.push((
                position,
                position + width - line.trim_end_matches('\n').chars().count(),
            ));
            position += width;
        }

        for span in spans {
            let tag_name = match span.kind {
                ConflictSpanKind::CurrentLines => TAG_CONFLICT_CURRENT,
                ConflictSpanKind::IncomingLines => TAG_CONFLICT_INCOMING,
                ConflictSpanKind::MarkerLine => TAG_CONFLICT_MARKER,
            };
            for line in span.start_line..span.end_line {
                if let Some(&(start_offset, end_offset)) = line_bounds.get(line) {
                    Self::apply_tag_by_offset(buffer, tag_name, start_offset, end_offset);
                }
            }
        }
    }

    /// Enables "Accept Current"/"Accept Incoming" exactly while the caret
    /// sits inside a well-formed conflict block of an editable note.
    fn update_conflict_actions(&self) {
        let imp = self.imp();
        let enabled = *imp.in_editor_view.borrow()
            && !*imp.editor_viewer_mode.borrow()
            && self.cursor_conflict_block().is_some();

        for name in [
            ACTION_CONFLICT_ACCEPT_CURRENT,
            ACTION_CONFLICT_ACCEPT_INCOMING,
        ] {
            if let Some(action) = self
                .lookup_action(name)
                .and_then(|action| action.downcast::<gio::SimpleAction>().ok())
            {
                action.set_enabled(enabled);
            }
        }
    }

    fn cursor_conflict_block(&self) -> Option<ConflictBlock> {
        let imp = self.imp();
        let buffer = imp.editor_view.buffer();
        let (start, end) = buffer.bounds();
        let text = buffer.text(&start, &end, true).to_string();
        let line = usize::try_from(buffer.iter_at_mark(&buffer.get_insert()).line()).ok()?;
        conflict_block_at_line(&text, line)
    }

    /// Resolves the block under the caret in favor of `side`: the losing
    /// side's lines and all three marker lines are deleted and the
    /// surviving side's text spliced back in, as a plain undoable edit so
    /// hand-tuned content inside the block is what gets kept.
    fn accept_conflict(&self, side: ConflictSide) {
        let imp = self.imp();
        if !*imp.in_editor_view.borrow() || *imp.editor_viewer_mode.borrow() {
            return;
        }

        let Some(block) = self.cursor_conflict_block() else {
            return;
        };
        let resolution = block.resolve(side);

        let buffer = imp.editor_view.buffer();
        let Ok(start_line) = i32::try_from(resolution.start_line) else {
            return;
        };
        let Some(mut delete_start) = buffer.iter_at_line(start_line) else {
            return;
        };
        // A block ending on the last line has no following line to start
        // from; deleting through the end iterator then covers it.
        let mut delete_end = if resolution.end_line >= buffer.line_count().max(0) as usize {
            buffer.end_iter()
        } else {
            buffer
                .iter_at_line(resolution.end_line as i32)
                .unwrap_or_else(|| buffer.end_iter())
        };
        buffer.delete(&mut delete_start, &mut delete_end);

        if !resolution.replacement.is_empty() {
            let mut insert_iter = buffer.iter_at_mark(&buffer.get_insert());
            buffer.insert(&mut insert_iter, &resolution.replacement);
        }

        let message = match side {
            ConflictSide::Current => i18n::accepted_current_changes(),
            ConflictSide::Incoming => i18n::accepted_incoming_changes(),
        };
        imp.sync_status_label.set_label(&message);
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
        [
            "- [ ] ", "* [ ] ", "+ [ ] ", "- [x] ", "* [x] ", "+ [x] ", "- [X] ", "* [X] ",
            "+ [X] ",
        ]
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
                    Self::apply_tag_by_offset(
                        buffer,
                        TAG_SYNTAX,
                        line_start_offset + start,
                        line_start_offset + start + 2,
                    );
                    Self::apply_tag_by_offset(
                        buffer,
                        TAG_BOLD,
                        line_start_offset + start + 2,
                        line_start_offset + char_index,
                    );
                    Self::apply_tag_by_offset(
                        buffer,
                        TAG_SYNTAX,
                        line_start_offset + char_index,
                        line_start_offset + char_index + 2,
                    );
                } else {
                    bold_start = Some(char_index);
                }
                index += 2;
                continue;
            }

            if ch == '*' {
                if let Some(start) = italic_start.take() {
                    Self::apply_tag_by_offset(
                        buffer,
                        TAG_SYNTAX,
                        line_start_offset + start,
                        line_start_offset + start + 1,
                    );
                    Self::apply_tag_by_offset(
                        buffer,
                        TAG_ITALIC,
                        line_start_offset + start + 1,
                        line_start_offset + char_index,
                    );
                    Self::apply_tag_by_offset(
                        buffer,
                        TAG_SYNTAX,
                        line_start_offset + char_index,
                        line_start_offset + char_index + 1,
                    );
                } else {
                    italic_start = Some(char_index);
                }
                index += 1;
                continue;
            }

            if line[byte_index..].starts_with("~~") {
                if let Some(start) = strike_start.take() {
                    Self::apply_tag_by_offset(
                        buffer,
                        TAG_SYNTAX,
                        line_start_offset + start,
                        line_start_offset + start + 2,
                    );
                    Self::apply_tag_by_offset(
                        buffer,
                        TAG_STRIKETHROUGH,
                        line_start_offset + start + 2,
                        line_start_offset + char_index,
                    );
                    Self::apply_tag_by_offset(
                        buffer,
                        TAG_SYNTAX,
                        line_start_offset + char_index,
                        line_start_offset + char_index + 2,
                    );
                } else {
                    strike_start = Some(char_index);
                }
                index += 2;
                continue;
            }

            if ch == '`' {
                if let Some(start) = code_start.take() {
                    Self::apply_tag_by_offset(
                        buffer,
                        TAG_SYNTAX,
                        line_start_offset + start,
                        line_start_offset + start + 1,
                    );
                    Self::apply_tag_by_offset(
                        buffer,
                        TAG_CODE,
                        line_start_offset + start + 1,
                        line_start_offset + char_index,
                    );
                    Self::apply_tag_by_offset(
                        buffer,
                        TAG_SYNTAX,
                        line_start_offset + char_index,
                        line_start_offset + char_index + 1,
                    );
                } else {
                    code_start = Some(char_index);
                }
                index += 1;
                continue;
            }

            if ch == '[' {
                if let Some(link) = Self::parse_link_at(&line[byte_index..]) {
                    Self::apply_tag_by_offset(
                        buffer,
                        TAG_SYNTAX,
                        line_start_offset + char_index,
                        line_start_offset + char_index + 1,
                    );
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
                Self::apply_tag_by_offset(
                    buffer,
                    TAG_CHECKED,
                    line_start_offset + item.marker_len,
                    line_start_offset + line.chars().count(),
                );
            }
        }
    }

    fn apply_tag_by_offset(
        buffer: &gtk::TextBuffer,
        tag_name: &str,
        start_offset: usize,
        end_offset: usize,
    ) {
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

    fn content_view_on_editor(view: &adw::NavigationView) -> bool {
        view.visible_page()
            .and_then(|page| page.tag())
            .is_some_and(|tag| tag == "editor")
    }

    pub(crate) fn show_grid_view(&self) {
        let imp = self.imp();
        *imp.in_editor_view.borrow_mut() = false;
        *imp.in_notes_grid_view.borrow_mut() = true;
        self.set_editor_only_actions_enabled(false);
        imp.current_entry_tags.borrow_mut().clear();
        self.update_window_title(None);
        let view = imp.content_view.get();
        if Self::content_view_on_editor(&view) {
            // Animated slide-back; the built-in edge-swipe gesture pops too.
            view.pop();
        }
        imp.app_header_bar.set_visible(true);
        imp.header_revealer.set_reveal_child(true);
        imp.main_page.set_spacing(8);
        imp.main_page.set_margin_top(MAIN_PAGE_MARGIN_NORMAL);
        imp.main_page.set_margin_bottom(MAIN_PAGE_MARGIN_NORMAL);
        imp.main_page.set_margin_start(MAIN_PAGE_MARGIN_NORMAL);
        imp.main_page.set_margin_end(MAIN_PAGE_MARGIN_NORMAL);
        imp.back_to_grid_button.set_visible(false);
        self.update_notes_search_reveal();

        // Put focus on the selected row so arrow keys / Enter / Delete work
        // immediately, no matter how we got back to the grid (button, Escape,
        // or the NavigationView edge-swipe pop).
        if let Some(button) = self.selected_note_button() {
            button.grab_focus();
        }
    }

    fn show_setup_page(&self) {
        let imp = self.imp();
        *imp.in_editor_view.borrow_mut() = false;
        *imp.in_notes_grid_view.borrow_mut() = false;
        self.set_editor_only_actions_enabled(false);
        imp.current_entry_tags.borrow_mut().clear();
        self.set_entry_modified(false);
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
        let view = imp.content_view.get();
        if !Self::content_view_on_editor(&view) {
            view.push(&*imp.editor_page);
        }
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

/// User-facing one-liner for a sync outcome (status line + tests).
fn sync_status_message(outcome: &SyncOutcome) -> String {
    match outcome.status.as_str() {
        "up_to_date" => "Journal up to date".to_string(),
        "pulled" => "Journal updated from remote".to_string(),
        "pushed" => "Journal pushed to remote".to_string(),
        "no_remote" => "No remote configured for this journal".to_string(),
        "no_branch" => "Journal has no branch yet".to_string(),
        "diverged" if outcome.conflicted_entry_ids.is_empty() => {
            "Journal diverged from remote".to_string()
        }
        "diverged" => format!(
            "Merge started: {} note{} need{} conflict resolution",
            outcome.conflicted_entry_ids.len(),
            if outcome.conflicted_entry_ids.len() == 1 {
                ""
            } else {
                "s"
            },
            if outcome.conflicted_entry_ids.len() == 1 {
                "s"
            } else {
                ""
            },
        ),
        other => format!("Sync state: {other}"),
    }
}

fn sync_conflict_toast_message(count: usize) -> String {
    i18n::conflicts_pending(count)
}

#[cfg(test)]
mod sync_message_tests {
    use super::*;

    fn outcome(status: &str, conflicts: &[&str]) -> SyncOutcome {
        SyncOutcome {
            status: status.to_string(),
            branch: Some("main".to_string()),
            ahead: Some(1),
            behind: Some(2),
            conflicted_entry_ids: conflicts.iter().map(|id| id.to_string()).collect(),
        }
    }

    #[test]
    fn messages_for_quiet_statuses() {
        assert_eq!(
            sync_status_message(&outcome("up_to_date", &[])),
            "Journal up to date"
        );
        assert_eq!(
            sync_status_message(&outcome("pulled", &[])),
            "Journal updated from remote"
        );
        assert_eq!(
            sync_status_message(&outcome("pushed", &[])),
            "Journal pushed to remote"
        );
        assert_eq!(
            sync_status_message(&outcome("no_remote", &[])),
            "No remote configured for this journal"
        );
        assert_eq!(
            sync_status_message(&outcome("no_branch", &[])),
            "Journal has no branch yet"
        );
    }

    #[test]
    fn diverged_message_counts_conflicts_and_handles_zero() {
        assert_eq!(
            sync_status_message(&outcome("diverged", &[])),
            "Journal diverged from remote"
        );
        assert_eq!(
            sync_status_message(&outcome("diverged", &["202601011200.md"])),
            "Merge started: 1 note needs conflict resolution"
        );
        assert_eq!(
            sync_status_message(&outcome(
                "diverged",
                &["202601011200.md", "202601011201.md"]
            )),
            "Merge started: 2 notes need conflict resolution"
        );
    }

    #[test]
    fn unknown_status_falls_back_to_verbatim_report() {
        assert_eq!(
            sync_status_message(&outcome("reconciled", &[])),
            "Sync state: reconciled"
        );
    }

    #[test]
    fn toast_message_pluralizes() {
        assert_eq!(
            sync_conflict_toast_message(1),
            "1 note needs conflict resolution"
        );
        assert_eq!(
            sync_conflict_toast_message(3),
            "3 notes need conflict resolution"
        );
    }
}

#[cfg(test)]
mod entry_id_for_day_tests {
    use super::*;

    fn summary(id: &str) -> EntrySummary {
        EntrySummary {
            entry_id: id.to_string(),
            tags: Vec::new(),
        }
    }

    #[test]
    fn picks_latest_entry_of_the_day() {
        let entries = vec![
            summary("202608240900.md"),
            summary("202608250800.md"),
            summary("202608251530.md"),
            summary("202608260001.md"),
        ];
        assert_eq!(
            PennaFrontendWindow::entry_id_for_day(&entries, "20260825"),
            Some("202608251530.md".to_string())
        );
    }

    #[test]
    fn single_entry_of_the_day() {
        let entries = vec![summary("202608240900.md"), summary("202608250800.md")];
        assert_eq!(
            PennaFrontendWindow::entry_id_for_day(&entries, "20260824"),
            Some("202608240900.md".to_string())
        );
    }

    #[test]
    fn no_entry_for_day() {
        let entries = vec![summary("202608240900.md"), summary("202608260001.md")];
        assert_eq!(
            PennaFrontendWindow::entry_id_for_day(&entries, "20260825"),
            None
        );
    }

    #[test]
    fn empty_list() {
        assert_eq!(PennaFrontendWindow::entry_id_for_day(&[], "20260825"), None);
    }
}
