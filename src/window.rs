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
    conflict_block_at_line, conflict_blocks, conflict_style_spans,
    unresolved_conflict_count, ConflictBlock, ConflictSide, ConflictSpanKind,
};
use crate::editor;
use crate::editor::{
    TAG_BLOCKQUOTE, TAG_BOLD, TAG_CHECKED, TAG_CODE, TAG_CODE_BLOCK, TAG_CONFLICT_CURRENT,
    TAG_CONFLICT_INCOMING, TAG_CONFLICT_MARKER, TAG_HEADING_1, TAG_HEADING_2, TAG_HEADING_3,
    TAG_HEADING_4, TAG_ITALIC, TAG_LINK, TAG_LIST_ITEM, TAG_LIST_MARKER, TAG_RULE,
    TAG_STRIKETHROUGH, TAG_SYNTAX,
};
use crate::engine::{
    ConnectResult, EngineMock, EngineOpError, EntrySnapshot, EntrySummary, JournalHandle,
    JournalKind, JournalStatus, SyncOutcome,
};
use crate::format;
use crate::gestures;
use crate::grid;
use crate::settings;
use crate::sync;

const HEADER_REVEAL_HOVER_Y: f64 = 56.0;
const MAIN_PAGE_MARGIN_NORMAL: i32 = 12;
const ACTION_CONFLICT_ACCEPT_CURRENT: &str = "conflict-accept-current";
const ACTION_CONFLICT_ACCEPT_INCOMING: &str = "conflict-accept-incoming";
const ACTION_CONFLICT_ACCEPT_BOTH: &str = "conflict-accept-both";

struct CheckboxItem {
    marker_len: usize,
    checked: bool,
}

struct LinkMatch {
    label_len: usize,
    total_len: usize,
}

/// Result of an off-main-thread save: the commit outcome plus the fresh
/// journal status, read together under a single engine lock.
struct SaveOutcome {
    result: Result<(), String>,
    status: Option<JournalStatus>,
}

/// Result of an off-main-thread disk reload: the reloaded entry count plus the
/// fresh fingerprint, read together under a single engine lock.
struct ReloadReport {
    count: Result<usize, String>,
    fingerprint: Option<u64>,
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
        pub clone_button: TemplateChild<gtk::Button>,
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
        #[template_child]
        pub conflict_banner: TemplateChild<adw::Banner>,

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
        pub pending_close: RefCell<bool>,
        pub conflict_widgets: RefCell<Vec<gtk::Box>>,
        pub conflicted_entry_ids: RefCell<Vec<String>>,
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
                clone_button: TemplateChild::default(),
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
                conflict_banner: TemplateChild::default(),
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
                editor_font_size_pt: RefCell::new(editor::EDITOR_FONT_SIZE_DEFAULT_PT),
                editor_viewer_mode: RefCell::default(),
                last_undo_generation: RefCell::default(),
                modified: RefCell::default(),
                pending_close: RefCell::default(),
                conflict_widgets: RefCell::default(),
                conflicted_entry_ids: RefCell::default(),
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

    impl WindowImpl for PennaFrontendWindow {
        fn close_request(&self) -> glib::Propagation {
            if *self.pending_close.borrow() {
                return glib::Propagation::Stop;
            }

            let needs_save = settings::get_bool(settings::SETTINGS_AUTO_SAVE_KEY)
                && *self.modified.borrow()
                && self.current_handle.borrow().is_some()
                && self.current_entry_id.borrow().is_some();

            if needs_save {
                *self.pending_close.borrow_mut() = true;
                if self.obj().save_current_entry() {
                    return glib::Propagation::Stop;
                }

                *self.pending_close.borrow_mut() = false;
            }

            glib::Propagation::Proceed
        }
    }

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

    pub fn refresh_entry_datetime_format(&self) {
        grid::refresh_notes_grid(self);

        // The headerbar shows the formatted entry date only while an entry is
        // open in the editor. On the notes grid the title stays "Diary", so
        // format changes must not touch it.
        if !*self.imp().in_editor_view.borrow() {
            return;
        }

        let entry_id = self.imp().current_entry_id.borrow().clone();
        self.update_window_title(entry_id.as_deref());
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
                let _ = window.save_current_entry();
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
                editor::adjust_editor_zoom(&window, 1);
            }
        ));
        self.add_action(&zoom_in);

        let zoom_out = gio::SimpleAction::new("zoom-out", None);
        zoom_out.connect_activate(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| {
                editor::adjust_editor_zoom(&window, -1);
            }
        ));
        self.add_action(&zoom_out);

        let toggle_viewer_mode = gio::SimpleAction::new("toggle-viewer-mode", None);
        toggle_viewer_mode.connect_activate(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| {
                editor::toggle_viewer_mode(&window);
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

        let accept_both_conflict = gio::SimpleAction::new(ACTION_CONFLICT_ACCEPT_BOTH, None);
        accept_both_conflict.connect_activate(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| {
                window.accept_conflict(ConflictSide::Both);
            }
        ));
        accept_both_conflict.set_enabled(false);
        self.add_action(&accept_both_conflict);

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

        editor::setup_editor_css(self);
        editor::setup_editor_tags(self);

        imp.connect_button.connect_clicked(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                window.choose_repo_folder();
            }
        ));

        imp.clone_button.connect_clicked(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                window.open_clone_dialog();
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
                editor::toggle_viewer_mode(&window);
            }
        ));

        imp.conflict_banner.connect_button_clicked(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                window.review_conflict();
            }
        ));

        imp.editor_view.buffer().connect_changed(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                window.cancel_pending_close();
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
        conflict_menu.append(Some(&i18n::accept_both_label()), Some("win.conflict-accept-both"));
        imp.editor_view.set_extra_menu(Some(&conflict_menu));

        imp.notes_search_entry.connect_search_changed(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                grid::refresh_notes_grid(&window);
                grid::update_notes_search_reveal(&window);
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

                grid::start_notes_search(&window, ch);
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

        // NavigationView's built-in forward swipe (a leftward touchpad swipe, or
        // the right-arrow / navigation.push action) pushes the next page. The
        // editor page is the only next page in this app: it is a persistent
        // child whose buffer still shows the last opened note, so hand it back
        // and let the NavigationView run the live, finger-tracked push.
        imp.content_view.connect_get_next_page(glib::clone!(
            #[weak(rename_to = window)]
            self,
            #[upgrade_or_else]
            || None::<adw::NavigationPage>,
            move |_view| -> Option<adw::NavigationPage> {
                let imp = window.imp();
                let on_grid = *imp.in_notes_grid_view.borrow();
                let has_entry = imp.current_entry_id.borrow().is_some();
                if on_grid && has_entry {
                    Some((*imp.editor_page).clone())
                } else {
                    None
                }
            }
        ));

        // A push lands the user in the editor. A button open already ran
        // show_editor_view before pushing, so skip it there; a forward swipe
        // pushes without touching app state, so resync here — mirroring the
        // popped handler above.
        imp.content_view.connect_pushed(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |view| {
                if PennaFrontendWindow::content_view_on_editor(view)
                    && !*window.imp().in_editor_view.borrow()
                {
                    window.show_editor_view();
                }
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
                    editor::adjust_editor_zoom(&window, 1);
                } else if dy > 0.0 {
                    editor::adjust_editor_zoom(&window, -1);
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
                    if let Some(button) = grid::selected_note_button(&window) {
                        let entry_id = button.widget_name().to_string();
                        if !entry_id.is_empty() {
                            window.open_entry(&entry_id);
                            return glib::Propagation::Stop;
                        }
                    }
                    return glib::Propagation::Proceed;
                }

                if matches!(keyval, gdk::Key::Delete | gdk::Key::KP_Delete) {
                    if grid::selected_note_button(&window).is_some() {
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

                if grid::move_note_focus(&window, direction) {
                    return glib::Propagation::Stop;
                }

                glib::Propagation::Proceed
            }
        ));
        imp.main_page.add_controller(grid_key_controller);

        gestures::install_editor_back_swipe(self);
        gestures::install_editor_back_swipe_touchpad(self);
        gestures::install_grid_reopen_swipe_touchpad(self);
        editor::load_editor_preferences(self);
        self.show_grid_view();
        self.initialize_repository_state();
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

        let connect_result = sync::connect_journal(self, &repo_path);

        match connect_result {
            Ok(result) => {
                let sync_message = match result.journal_kind {
                    JournalKind::New => "New diary initialized and connected",
                    JournalKind::Existing => "Existing journal connected",
                };
                let details = format!(
                    "{sync_message} | branch: {} | capabilities: {}",
                    result.current_branch,
                    result.capabilities.join(", ")
                );
                // A fresh diary has nothing to sync; an existing one immediately
                // runs the update flow against its remote.
                let run_initial_sync = result.journal_kind == JournalKind::Existing;
                self.apply_connected_journal(&result, &details, run_initial_sync);
            }
            Err(err) => {
                imp.setup_status_label.set_label(&err);
                imp.sync_status_label.set_label(&err);
            }
        }
    }

    /// Shared post-connect steps for both the connect and clone flows: register
    /// the handle, persist the repository path, update the status labels,
    /// refresh the grid, start the watchers, and show the main page.
    /// `run_initial_sync` triggers the update flow for a session that already
    /// has a remote to reconcile against.
    fn apply_connected_journal(
        &self,
        result: &ConnectResult,
        status_text: &str,
        run_initial_sync: bool,
    ) {
        let imp = self.imp();
        self.cancel_pending_close();
        self.autosave_if_modified();
        *imp.current_entry_id.borrow_mut() = None;
        imp.current_entry_tags.borrow_mut().clear();
        self.set_entry_modified(false);
        *imp.current_handle.borrow_mut() = Some(result.journal_handle);
        let _ =
            settings::set_str(settings::SETTINGS_REPOSITORY_PATH_KEY, &result.repo_path);
        imp.sync_status_label.set_label(status_text);
        imp.setup_status_label.set_label(status_text);
        grid::refresh_notes_grid(self);
        self.start_repo_watchers();
        self.show_main_page();
        self.show_grid_view();
        if run_initial_sync {
            self.run_sync_flow();
        }
    }

    /// Update flow for an already-connected session: fetch/push/merge via the
    /// engine's `sync_journal`, then surface whatever the merge produced.
    fn run_sync_flow(&self) {
        let imp = self.imp();
        let Some(handle) = *imp.current_handle.borrow() else {
            return;
        };

        let outcome = sync::sync_journal(self, handle);

        match outcome {
            Ok(outcome) => self.handle_sync_outcome(&outcome),
            Err(EngineOpError::AuthRequired { remote_url }) => {
                self.prompt_for_credential(&remote_url, |window| window.run_sync_flow())
            }
            Err(err) => {
                let message = err.to_display_string();
                imp.sync_status_label
                    .set_label(&i18n::sync_failed(&message));
                let toast = adw::Toast::new(&i18n::sync_failed(&message));
                toast.set_priority(adw::ToastPriority::High);
                imp.toast_overlay.add_toast(toast);
            }
        }
    }

    /// When the engine reports that `remote_url` needs a credential, prompt the
    /// user for it, store it in the platform secret store, then re-run
    /// `on_stored`. Cancelling (or closing the prompt) does nothing, leaving
    /// the failed status in place.
    fn prompt_for_credential(
        &self,
        remote_url: &str,
        on_stored: impl FnOnce(&PennaFrontendWindow) + 'static,
    ) {
        let for_display = remote_url.to_string();
        let for_closure = remote_url.to_string();
        self.prompt_for_token(
            &for_display,
            glib::clone!(
                #[weak(rename_to = window)]
                self,
                move |token: Option<String>| {
                    let Some(token) = token else {
                        return;
                    };
                    let _ = sync::store_credential(&window, &for_closure, &token);
                    on_stored(&window);
                }
            ),
        );
    }

    /// Modal prompt for a credential (e.g. an HTTPS token / personal access
    /// token) for `remote_url`. Invokes `on_done` exactly once with the entered
    /// token, or `None` if the user cancels, leaves it blank, or closes the
    /// window.
    fn prompt_for_token(&self, remote_url: &str, on_done: impl FnOnce(Option<String>) + 'static) {
        let dialog = adw::Dialog::new();
        dialog.set_title(&i18n::auth_required_title());
        dialog.set_content_width(460);
        dialog.set_content_height(230);

        let toolbar = adw::ToolbarView::new();
        let header = adw::HeaderBar::new();
        header.add_css_class("flat");
        header.set_title_widget(Some(&adw::WindowTitle::new(
            i18n::auth_required_title().as_str(),
            "",
        )));
        toolbar.add_top_bar(&header);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.set_margin_start(12);
        content.set_margin_end(12);
        toolbar.set_content(Some(&content));
        dialog.set_child(Some(&toolbar));

        let hint = gtk::Label::new(Some(&i18n::auth_hint()));
        hint.set_wrap(true);
        hint.set_halign(gtk::Align::Start);
        content.append(&hint);

        let url_label = gtk::Label::new(Some(remote_url));
        url_label.set_wrap(true);
        url_label.set_halign(gtk::Align::Start);
        url_label.add_css_class("dim-label");
        url_label.set_selectable(true);
        content.append(&url_label);

        let entry = gtk::Entry::new();
        entry.set_visibility(false);
        entry.set_placeholder_text(Some(i18n::token_placeholder().as_str()));
        content.append(&entry);

        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        buttons.set_halign(gtk::Align::End);
        let cancel_button = gtk::Button::with_label(i18n::cancel().as_str());
        let save_button = gtk::Button::with_label(i18n::save_token().as_str());
        save_button.add_css_class("suggested-action");
        buttons.append(&cancel_button);
        buttons.append(&save_button);
        content.append(&buttons);

        // The buttons record the entered token; the one-shot `on_done` (boxed,
        // because the `closed` handler is an `Fn` closure) is consumed exactly
        // once when the dialog closes via `RefCell::take`. Left unset (window
        // closed directly) there is no decision, so `on_done` never runs.
        let decision = Rc::new(RefCell::new(None::<Option<String>>));
        type TokenOnDoneCell = Rc<RefCell<Option<Box<dyn FnOnce(Option<String>)>>>>;
        let on_done_cell: TokenOnDoneCell =
            Rc::new(RefCell::new(Some(Box::new(on_done))));

        let decision_save = decision.clone();
        let entry_save = entry.clone();
        let dialog_save = dialog.clone();
        save_button.connect_clicked(move |_| {
            let text = entry_save.text().to_string();
            let token = if text.trim().is_empty() {
                None
            } else {
                Some(text)
            };
            *decision_save.borrow_mut() = Some(token);
            dialog_save.close();
        });

        let decision_cancel = decision.clone();
        let dialog_cancel = dialog.clone();
        cancel_button.connect_clicked(move |_| {
            *decision_cancel.borrow_mut() = Some(None);
            dialog_cancel.close();
        });

        let decision_done = decision.clone();
        let on_done_done = on_done_cell.clone();
        dialog.connect_closed(move |_| {
            if let Some(token) = decision_done.borrow_mut().take() {
                if let Some(callback) = on_done_done.borrow_mut().take() {
                    callback(token);
                }
            }
        });

        dialog.present(Some(self));
        entry.grab_focus();
    }

    fn handle_sync_outcome(&self, outcome: &SyncOutcome) {
        let imp = self.imp();
        grid::refresh_notes_grid(self);

        let message = sync_status_message(outcome);
        imp.sync_status_label.set_label(&message);

        self.set_conflict_banner(&outcome.conflicted_entry_ids);
    }

    /// Persistent conflict notice: the banner stays revealed while any entry
    /// still has an unresolved index conflict, with a button that jumps to the
    /// first one. Replaces the transient "conflicts found" toast.
    pub(crate) fn set_conflict_banner(&self, entry_ids: &[String]) {
        let imp = self.imp();
        if entry_ids.is_empty() {
            imp.conflicted_entry_ids.borrow_mut().clear();
            imp.conflict_banner.set_revealed(false);
            return;
        }
        imp.conflicted_entry_ids.replace(entry_ids.to_vec());
        imp.conflict_banner
            .set_title(&i18n::conflicts_pending(entry_ids.len()));
        imp.conflict_banner
            .set_button_label(Some(&i18n::conflict_review_action()));
        imp.conflict_banner.set_revealed(true);
    }

    /// Opens the first still-conflicted note so the user can resolve it.
    pub(crate) fn review_conflict(&self) {
        let first = self.imp().conflicted_entry_ids.borrow().first().cloned();
        if let Some(id) = first {
            self.open_entry(&id);
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

        let merge_pending =
            sync::journal_status(self, handle).is_some_and(|status| status.merge_in_progress);

        if !merge_pending {
            return;
        }

        let outcome = sync::sync_journal(self, handle);
        match outcome {
            Ok(outcome) if outcome.conflicted_entry_ids.is_empty() => {
                self.handle_sync_outcome(&outcome);
                let toast = adw::Toast::new(&i18n::all_conflicts_resolved());
                toast.set_priority(adw::ToastPriority::High);
                imp.toast_overlay.add_toast(toast);
            }
            Ok(_) => {}
            Err(EngineOpError::AuthRequired { remote_url }) => {
                self.prompt_for_credential(&remote_url, |window| {
                    window.maybe_conclude_merge()
                })
            }
            Err(err) => imp
                .sync_status_label
                .set_label(&i18n::sync_failed(&err.to_display_string())),
        }
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

    pub(crate) fn open_entry(&self, entry_id: &str) {
        if self.load_entry(entry_id) {
            self.show_editor_view();
        }
    }

    /// Loads a note into the editor buffer and syncs entry state without
    /// navigating. Returns false if the note cannot be loaded (no repository
    /// connected, or it has since been deleted). A button open follows up with
    /// show_editor_view; a forward swipe lets the NavigationView push the
    /// editor page itself.
    fn load_entry(&self, entry_id: &str) -> bool {
        let imp = self.imp();
        let Some(handle) = *imp.current_handle.borrow() else {
            return false;
        };

        let Some(entry) = sync::get_entry(self, handle, entry_id) else {
            return false;
        };

        let same_entry = imp.current_entry_id.borrow().as_deref() == Some(entry_id);
        self.cancel_pending_close();
        if !same_entry {
            self.autosave_if_modified();
        }

        *imp.current_entry_id.borrow_mut() = Some(entry_id.to_string());
        *imp.current_entry_tags.borrow_mut() = entry.tags.clone();
        self.update_window_title(Some(entry_id));
        self.clear_conflict_widgets();
        imp.editor_view.buffer().set_text(&entry.content);
        // Loading content fires the change handler above; the freshly loaded
        // note is by definition saved.
        self.set_entry_modified(false);
        editor::apply_editor_mode(self);
        self.apply_markdown_styling();
        self.refresh_conflict_widgets();
        true
    }

    /// Reopen the most recently opened note — the one tracked in
    /// `current_entry_id` (shown in the editor before the user last returned
    /// to the grid). No-op if none is tracked, or if it has since been deleted
    /// (get_entry returns None). Used by the scrollable-grid forward swipe,
    /// which NavigationView's built-in live swipe cannot drive.
    pub(crate) fn reopen_last_entry(&self) {
        let Some(entry_id) = self.imp().current_entry_id.borrow().clone() else {
            return;
        };
        self.open_entry(&entry_id);
    }

    fn autosave_if_modified(&self) {
        let imp = self.imp();
        if *imp.modified.borrow() && settings::get_bool(settings::SETTINGS_AUTO_SAVE_KEY) {
            let _ = self.save_current_entry();
        }
    }

    fn cancel_pending_close(&self) {
        if *self.imp().pending_close.borrow() {
            *self.imp().pending_close.borrow_mut() = false;
        }
    }

    fn finish_pending_close(&self) {
        let imp = self.imp();
        if !*imp.pending_close.borrow() {
            return;
        }

        *imp.pending_close.borrow_mut() = false;
        self.close();
    }

    fn save_current_entry(&self) -> bool {
        let imp = self.imp();
        let Some(handle) = *imp.current_handle.borrow() else {
            imp.sync_status_label
                .set_label(&i18n::connect_repository_before_saving());
            return false;
        };

        let Some(entry_id) = imp.current_entry_id.borrow().clone() else {
            imp.sync_status_label.set_label(&i18n::no_entry_selected());
            return false;
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
            return false;
        }

        let tags = imp.current_entry_tags.borrow().clone();

        // The git commit runs off the main thread so the window never freezes;
        // the result is applied on the main thread once it lands.
        imp.sync_status_label.set_label(&i18n::saving());
        let save_entry_id = entry_id.clone();
        sync::offload::<SaveOutcome, _, _>(
            self,
            move |engine| {
                let mut engine = engine.lock().unwrap();
                let result = engine.entry_save(handle, &save_entry_id, &content, &tags);
                // Grab the fresh status under the same lock so the status bar
                // can update without a second main-thread git read.
                let status = engine.journal_status(handle);
                SaveOutcome { result, status }
            },
            glib::clone!(
                #[weak(rename_to = window)]
                self,
                move |outcome: SaveOutcome| match outcome.result {
                    Ok(()) => {
                        // Ignore a save that landed after the user moved on to
                        // another note.
                        if window
                            .imp()
                            .current_entry_id
                            .borrow()
                            .as_deref()
                            != Some(entry_id.as_str())
                        {
                            window.finish_pending_close();
                            return;
                        }
                        window.set_entry_modified(false);
                        window.imp().sync_status_label.set_label(&i18n::saved());
                        // Saving content leaves the (hidden) grid rows
                        // unchanged, so no grid rebuild. Only conclude a
                        // merge if one is actually in flight.
                        if let Some(status) = &outcome.status {
                            if status.merge_in_progress {
                                window.maybe_conclude_merge();
                            }
                        }
                        window.finish_pending_close();
                    }
                    Err(err) => {
                        window.imp().sync_status_label.set_label(&err);
                        window.finish_pending_close();
                    }
                }
            ),
        );

        true
    }

    fn create_new_entry(&self) {
        let imp = self.imp();
        let Some(handle) = *imp.current_handle.borrow() else {
            return;
        };

        self.cancel_pending_close();

        // One note per day: if today already has an entry, redirect to it
        // instead of creating a duplicate.
        let today = chrono::Local::now().format("%Y%m%d").to_string();
        let existing = Self::entry_id_for_day(&sync::list_entries(self, handle), &today);

        if let Some(existing) = existing {
            if imp.current_entry_id.borrow().as_deref() == Some(existing.as_str()) {
                let toast = adw::Toast::new(&i18n::opened_todays_note());
                imp.toast_overlay.add_toast(toast);
                self.show_editor_view();
                return;
            }

            // Switching to another note saves the one being left first.
            self.open_entry(&existing);
            return;
        }

        self.autosave_if_modified();

        let record = match sync::create_entry_new(self, handle) {
            Ok(record) => record,
            Err(err) => {
                imp.sync_status_label.set_label(&err);
                return;
            }
        };

        *imp.current_entry_id.borrow_mut() = Some(record.entry_id.clone());
        *imp.current_entry_tags.borrow_mut() = record.tags.clone();
        self.update_window_title(Some(&record.entry_id));
        self.clear_conflict_widgets();
        imp.editor_view.buffer().set_text(&record.content);
        self.set_entry_modified(false);
        editor::apply_editor_mode(self);
        self.apply_markdown_styling();
        self.show_editor_view();
        self.refresh_conflict_widgets();
        grid::refresh_notes_grid(self);
    }

    fn delete_current_entry(&self) {
        let imp = self.imp();
        self.cancel_pending_close();
        let Some(handle) = *imp.current_handle.borrow() else {
            imp.sync_status_label
                .set_label(&i18n::connect_repository_before_deleting());
            return;
        };
        let entry_id = if *imp.in_notes_grid_view.borrow() {
            grid::selected_note_button(self)
                .map(|button| button.widget_name().to_string())
                .or_else(|| imp.current_entry_id.borrow().clone())
        } else {
            imp.current_entry_id.borrow().clone()
        };

        let Some(entry_id) = entry_id else {
            imp.sync_status_label.set_label(&i18n::no_note_selected());
            return;
        };

        let snapshot = match sync::delete_entry_with_snapshot(self, handle, &entry_id) {
            Ok(snapshot) => snapshot,
            Err(err) => {
                imp.sync_status_label.set_label(&err);
                return;
            }
        };

        *imp.current_entry_id.borrow_mut() = None;
        imp.current_entry_tags.borrow_mut().clear();
        self.set_entry_modified(false);
        self.show_grid_view();
        grid::refresh_notes_grid(self);
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

                let result = sync::restore_entry(&window, handle, &snapshot);

                match result {
                    Ok(record) => {
                        window_imp.sync_status_label.set_label(&i18n::note_restored());
                        grid::refresh_notes_grid(&window);
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

    /// Entry point for the "From a server" option (welcome page and settings):
    /// open a modal to enter a repository URL and a destination folder, then
    /// clone it into the app.
    pub fn open_clone_dialog(&self) {
        let dialog = adw::Dialog::new();
        dialog.set_title(&i18n::clone_journal_title());
        dialog.set_content_width(520);
        dialog.set_content_height(380);

        let toolbar = adw::ToolbarView::new();
        let header = adw::HeaderBar::new();
        header.add_css_class("flat");
        header.set_title_widget(Some(&adw::WindowTitle::new(
            i18n::clone_journal_title().as_str(),
            "",
        )));
        let cancel_button = gtk::Button::with_label(i18n::cancel().as_str());
        header.pack_start(&cancel_button);
        let clone_button = gtk::Button::with_label(i18n::clone_action_label().as_str());
        clone_button.add_css_class("suggested-action");
        header.pack_end(&clone_button);
        toolbar.add_top_bar(&header);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.set_margin_start(12);
        content.set_margin_end(12);
        toolbar.set_content(Some(&content));
        dialog.set_child(Some(&toolbar));

        let home_str = glib::home_dir().as_path().to_string_lossy().to_string();

        let url_label = gtk::Label::new(Some(&i18n::clone_url_label()));
        url_label.set_halign(gtk::Align::Start);
        content.append(&url_label);

        let url_entry = gtk::Entry::new();
        url_entry.set_placeholder_text(Some(i18n::clone_url_placeholder().as_str()));
        url_entry.set_hexpand(true);
        content.append(&url_entry);

        let save_to_label = gtk::Label::new(Some(&i18n::clone_save_to_label()));
        save_to_label.set_halign(gtk::Align::Start);
        content.append(&save_to_label);

        let dest_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        dest_box.set_hexpand(true);
        let dest_label = gtk::Label::new(Some(&home_str));
        dest_label.set_wrap(true);
        dest_label.set_selectable(true);
        dest_label.set_hexpand(true);
        dest_label.set_halign(gtk::Align::Start);
        dest_box.append(&dest_label);
        let dest_browse = gtk::Button::with_label(i18n::browse_label().as_str());
        dest_box.append(&dest_browse);
        content.append(&dest_box);

        let status_label = gtk::Label::new(None);
        status_label.set_wrap(true);
        status_label.set_halign(gtk::Align::Start);
        content.append(&status_label);

        let parent_dir = Rc::new(RefCell::new(home_str));

        // Browse: pick the parent folder the clone will be created under.
        dest_browse.connect_clicked(glib::clone!(
            #[weak(rename_to = window)]
            self,
            #[strong]
            parent_dir,
            move |_btn| {
                let dest_label_click = dest_label.clone();
                let picker = gtk::FileDialog::builder()
                    .title(i18n::choose_clone_destination().as_str())
                    .accept_label("Select")
                    .modal(true)
                    .build();
                picker.select_folder(
                    Some(&window),
                    None::<&gio::Cancellable>,
                    glib::clone!(
                        #[strong]
                        parent_dir,
                        move |result| {
                            if let Ok(file) = result {
                                if let Some(path) = file.path() {
                                    let chosen = path.to_string_lossy().to_string();
                                    *parent_dir.borrow_mut() = chosen.clone();
                                    dest_label_click.set_text(&chosen);
                                }
                            }
                        }
                    ),
                );
            }
        ));

        let dialog_for_cancel = dialog.clone();
        cancel_button.connect_clicked(move |_| {
            dialog_for_cancel.close();
        });

        let dialog_for_clone = dialog.clone();
        clone_button.connect_clicked(glib::clone!(
            #[weak(rename_to = window)]
            self,
            #[strong]
            parent_dir,
            move |_btn| {
                let url = url_entry.text().to_string();
                if url.trim().is_empty() {
                    status_label.set_text(&i18n::clone_url_required());
                    return;
                }
                let parent = parent_dir.borrow().clone();
                let dir_name = PennaFrontendWindow::repo_name_from_url(&url);
                dialog_for_clone.close();
                window.run_clone_flow(url, parent, dir_name);
            }
        ));

        dialog.present(Some(self));
    }

    /// Clone flow: clone from `remote_url` into `parent_dir/dir_name`. On a
    /// `CredentialsRequired` failure, prompt for a token, store it, and retry.
    /// On success, wire the new journal into the window like a fresh connect
    /// (no initial sync — a clone is already in sync with its remote).
    fn run_clone_flow(&self, remote_url: String, parent_dir: String, dir_name: String) {
        let result = sync::clone_journal(self, &remote_url, &parent_dir, &dir_name);
        match result {
            Ok(result) => {
                let details = format!(
                    "{} | branch: {}",
                    i18n::cloned_connected(&remote_url),
                    result.current_branch
                );
                self.apply_connected_journal(&result, &details, false);
            }
            Err(EngineOpError::AuthRequired { remote_url: auth_url }) => {
                self.prompt_for_credential(&auth_url, move |window| {
                    window.run_clone_flow(remote_url, parent_dir, dir_name);
                });
            }
            Err(err) => {
                let message = err.to_display_string();
                let imp = self.imp();
                let toast = adw::Toast::new(&i18n::clone_failed(&message));
                toast.set_priority(adw::ToastPriority::High);
                imp.toast_overlay.add_toast(toast);
            }
        }
    }

    /// Derive a clone-target folder name from a repository URL: the last
    /// non-empty path segment with a trailing `.git` stripped.
    fn repo_name_from_url(url: &str) -> String {
        let trimmed = url.trim();
        let tail = trimmed
            .rsplit_once("://")
            .map(|(_, rest)| rest)
            .unwrap_or(trimmed);
        let last = tail.rsplit('/').next().unwrap_or(trimmed);
        let name = last.strip_suffix(".git").unwrap_or(last);
        if name.is_empty() {
            "diary".to_string()
        } else {
            name.to_string()
        }
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

        let initial_fingerprint = sync::entries_fingerprint(self, handle).ok();
        *imp.last_entries_fingerprint.borrow_mut() = initial_fingerprint;

        self.start_entries_monitor();
        self.start_refresh_fallback_timer();
    }

    fn start_entries_monitor(&self) {
        let imp = self.imp();
        let Some(handle) = *imp.current_handle.borrow() else {
            return;
        };

        let watch_path = sync::entries_directory(self, handle);

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

        // The fingerprint read runs off the main thread so the periodic check
        // never freezes the UI; a genuine change is reloaded when it lands.
        sync::offload::<Result<u64, String>, _, _>(
            self,
            move |engine| {
                let engine = engine.lock().unwrap();
                engine.entries_fingerprint(handle)
            },
            glib::clone!(
                #[weak(rename_to = window)]
                self,
                move |fingerprint| match fingerprint {
                    Ok(current) => {
                        if window
                            .imp()
                            .last_entries_fingerprint
                            .borrow()
                            .as_ref()
                            != Some(&current)
                        {
                            window.reload_entries_from_disk("Detected external update.");
                        }
                    }
                    Err(err) => {
                        window.imp().sync_status_label
                            .set_label(&format!("Unable to inspect repository files: {err}"));
                    }
                }
            ),
        );
    }

    fn reload_entries_from_disk(&self, status_prefix: &str) {
        let imp = self.imp();
        let Some(handle) = *imp.current_handle.borrow() else {
            return;
        };

        // Our own save writes the entry file and trips this monitor, so the
        // reload + fingerprint runs off the main thread (no freeze); the grid
        // and status update once it lands.
        let status_prefix = status_prefix.to_string();
        sync::offload::<ReloadReport, _, _>(
            self,
            move |engine| {
                let mut engine = engine.lock().unwrap();
                let count = engine.reload_entries(handle);
                let fingerprint = engine.entries_fingerprint(handle).ok();
                ReloadReport { count, fingerprint }
            },
            glib::clone!(
                #[weak(rename_to = window)]
                self,
                move |report: ReloadReport| match report.count {
                    Ok(count) => {
                        window.apply_reload_report(count, report.fingerprint, &status_prefix);
                    }
                    Err(err) => {
                        window.imp().sync_status_label
                            .set_label(&format!("Unable to reload entries from disk: {err}"));
                    }
                }
            ),
        );
    }

    fn apply_reload_report(&self, count: usize, fingerprint: Option<u64>, status_prefix: &str) {
        let imp = self.imp();
        grid::refresh_notes_grid(self);
        *imp.last_entries_fingerprint.borrow_mut() = fingerprint;
        imp.sync_status_label
            .set_label(&format!("{status_prefix} Entries: {count}"));
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

        let available_tags = Rc::new(RefCell::new(sync::list_tags(self, handle)));
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
                    chips_flow.append(&grid::build_tag_chip(&tag));
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
                    let result = if attach {
                        sync::add_tag(&window, handle, &entry_id, tag)
                    } else {
                        sync::remove_tag(&window, handle, &entry_id, tag)
                    };

                    if let Ok(tags) = result {
                        *current_tags.borrow_mut() = tags.clone();
                        *window.imp().current_entry_tags.borrow_mut() = tags;
                        grid::refresh_notes_grid(&window);
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

                    let result = sync::add_tag(&window, handle, &entry_id, &tag);

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
                        grid::refresh_notes_grid(&window);
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
            #[strong]
            dialog,
            move |_, keyval, _, _| {
                if keyval == gdk::Key::Escape {
                    dialog.close();
                    return glib::Propagation::Stop;
                }

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
            #[strong]
            dialog,
            move |_, keyval, _, _| {
                if keyval == gdk::Key::Escape {
                    dialog.close();
                    return glib::Propagation::Stop;
                }

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

        // AdwDialog has no built-in Escape handling and this dialog has no
        // close button. The search-entry and list controllers above handle
        // Esc for their focus states (they run before a child's default
        // key handling); this dialog-level controller is the backstop for
        // any other focusable. Focus returns to the editor on close.
        let dialog_for_esc = dialog.clone();
        let esc_keys = gtk::EventControllerKey::new();
        esc_keys.connect_key_pressed(move |_, keyval, _, _| {
            if keyval == gdk::Key::Escape {
                dialog_for_esc.close();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        dialog.add_controller(esc_keys);

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
        // Apply per line through iterators, not absolute char offsets: the
        // inline resolution buttons are child anchors occupying buffer
        // positions, which shifts every character after them relative to the
        // plain `text` we parsed from.
        for span in conflict_style_spans(text) {
            let tag_name = match span.kind {
                ConflictSpanKind::CurrentLines => TAG_CONFLICT_CURRENT,
                ConflictSpanKind::IncomingLines => TAG_CONFLICT_INCOMING,
                ConflictSpanKind::MarkerLine => TAG_CONFLICT_MARKER,
            };
            for line in span.start_line..span.end_line {
                if let Ok(line_no) = i32::try_from(line) {
                    Self::apply_tag_to_line(buffer, tag_name, line_no);
                }
            }
        }
    }

    /// Applies `tag_name` across a whole line — from its first character up to
    /// (excluding) the line break — using the buffer's own line structure so
    /// that child anchors cannot skew the covered range.
    fn apply_tag_to_line(buffer: &gtk::TextBuffer, tag_name: &str, line_no: i32) {
        let Some(start) = buffer.iter_at_line(line_no) else {
            return;
        };
        let Some(mut end) = buffer.iter_at_line(line_no) else {
            return;
        };
        end.forward_to_line_end();
        buffer.apply_tag_by_name(tag_name, &start, &end);
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
            ACTION_CONFLICT_ACCEPT_BOTH,
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
            ConflictSide::Both => i18n::accepted_both_changes(),
        };
        imp.sync_status_label.set_label(&message);
    }

    /// Replaces the inline resolution buttons with a fresh set, one per
    /// unresolved conflict block currently in the buffer.
    ///
    /// Each block gets two zero-width child anchors: a "This device" button at
    /// the end of its current side's last line and an "Other device" button at
    /// the end of its incoming side's last line, so each tinted block is
    /// self-contained. Editing the text (the third way) leaves the buttons in
    /// place while the tinted tags reflow around the cursor.
    /// Anchors and widgets are cleared on `set_text`, so this is only called
    /// when a note is opened, the editor mode changes, or a block is resolved —
    /// never on a keystroke.
    pub(crate) fn refresh_conflict_widgets(&self) {
        self.clear_conflict_widgets();

        let imp = self.imp();
        if !*imp.in_editor_view.borrow() || *imp.editor_viewer_mode.borrow() {
            return;
        }

        let buffer = imp.editor_view.buffer();
        let (start, end) = buffer.bounds();
        let text = buffer.text(&start, &end, true).to_string();
        let blocks = conflict_blocks(&text);

        for (index, block) in blocks.iter().enumerate() {
            let current_button = gtk::Button::with_label(&i18n::conflict_this_device_label());
            current_button.add_css_class("flat");
            current_button.add_css_class("conflict-current-button");
            current_button.connect_clicked(glib::clone!(
                #[weak(rename_to = window)]
                self,
                move |_| window.resolve_conflict_at_index(index, ConflictSide::Current)
            ));
            self.attach_conflict_button(block.current_anchor_line, &current_button);

            let both_button = gtk::Button::with_label(&i18n::conflict_keep_both_label());
            both_button.add_css_class("flat");
            both_button.add_css_class("conflict-both-button");
            both_button.connect_clicked(glib::clone!(
                #[weak(rename_to = window)]
                self,
                move |_| window.resolve_conflict_at_index(index, ConflictSide::Both)
            ));
            self.attach_conflict_button(block.separator_anchor_line, &both_button);

            let incoming_button = gtk::Button::with_label(&i18n::conflict_other_device_label());
            incoming_button.add_css_class("flat");
            incoming_button.add_css_class("conflict-incoming-button");
            incoming_button.connect_clicked(glib::clone!(
                #[weak(rename_to = window)]
                self,
                move |_| window.resolve_conflict_at_index(index, ConflictSide::Incoming)
            ));
            self.attach_conflict_button(block.incoming_anchor_line, &incoming_button);
        }
    }

    /// Removes every anchored resolution button from the view and clears the
    /// tracking list. Call this before mutating the buffer: deleting a range
    /// that still holds our child anchors makes GTK remove the child
    /// mid-mutation, which queries the selection bounds on a half-deleted
    /// buffer and segfaults.
    fn clear_conflict_widgets(&self) {
        let imp = self.imp();
        for holder in imp.conflict_widgets.borrow().iter() {
            imp.editor_view.remove(holder);
        }
        imp.conflict_widgets.borrow_mut().clear();
    }

    /// Anchors `button` to the end of `line` via a zero-width child anchor,
    /// wrapping it in the shared resolution-holder styling and tracking it in
    /// `conflict_widgets` so the next refresh can remove it.
    fn attach_conflict_button(&self, line: usize, button: &gtk::Button) {
        // The buttons sit inside the text view, which otherwise shows its
        // insertion (I-beam) cursor over them; keep the plain arrow instead.
        button.set_cursor_from_name(Some("default"));
        let imp = self.imp();
        let buffer = imp.editor_view.buffer();
        let Ok(line_no) = i32::try_from(line) else {
            return;
        };
        let Some(mut iter) = buffer.iter_at_line(line_no) else {
            return;
        };
        iter.forward_to_line_end();
        let anchor = buffer.create_child_anchor(&mut iter);

        let holder = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        holder.add_css_class("conflict-resolve");
        holder.set_margin_start(8);
        holder.append(button);
        imp.editor_view.add_child_at_anchor(&holder, &anchor);
        imp.conflict_widgets.borrow_mut().push(holder);
    }

    /// Resolves the `index`-th unresolved conflict block, keeping `side`'s
    /// lines, then restyles and re-lays-out the remaining resolution buttons.
    fn resolve_conflict_at_index(&self, index: usize, side: ConflictSide) {
        // Resolve off the button's own `clicked` callback: the final widget
        // refresh drops the last ref to the clicked button, and freeing a
        // widget whose signal is still on the stack crashes. Deferring to the
        // next main-loop tick lets `clicked` unwind first.
        glib::idle_add_local_once(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move || {
                window.apply_conflict_resolution(index, side);
            },
        ));
    }

    fn apply_conflict_resolution(&self, index: usize, side: ConflictSide) {
        let imp = self.imp();
        if !*imp.in_editor_view.borrow() || *imp.editor_viewer_mode.borrow() {
            return;
        }

        // Detach the buttons before touching the buffer so the delete below
        // never destroys a child anchor mid-mutation (see clear_conflict_widgets).
        self.clear_conflict_widgets();

        let buffer = imp.editor_view.buffer();
        let (start, end) = buffer.bounds();
        let text = buffer.text(&start, &end, true).to_string();
        let Some(block) = conflict_blocks(&text).into_iter().nth(index) else {
            return;
        };
        let resolution = block.resolve(side);

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
            ConflictSide::Both => i18n::accepted_both_changes(),
        };
        imp.sync_status_label.set_label(&message);

        self.apply_markdown_styling();
        self.refresh_conflict_widgets();
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
        self.cancel_pending_close();
        self.autosave_if_modified();
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
        grid::update_notes_search_reveal(self);

        // Put focus on the selected row so arrow keys / Enter / Delete work
        // immediately, no matter how we got back to the grid (button, Escape,
        // or the NavigationView edge-swipe pop).
        if let Some(button) = grid::selected_note_button(self) {
            button.grab_focus();
        }
    }

    fn show_setup_page(&self) {
        let imp = self.imp();
        self.cancel_pending_close();
        self.autosave_if_modified();
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
        grid::update_notes_search_reveal(self);
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
