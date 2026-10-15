//! Tags dialog: editing the current entry's tags via interactive, wrapping
//! pills. Each pill shows the tag name and a small action button, with the
//! container coloured by its state:
//!   - attached to this note: solid fill, cross button (removes the tag)
//!   - used by other notes: transparent with a solid outline, plus button
//!   - brand new (typing a name that no tag uses): transparent, dashed outline,
//!     plus button (creates the tag)
//!
//! Pills are wrapped manually into horizontal rows because `GtkFlowBox`
//! collapses to one-per-line (each full width) as soon as a single tag's
//! natural width is large — exactly what happens with long tag names.
//!
//! Presentation order: attached tags, then the create-new pill, then
//! existing-but-unselected tags.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, pango};

use crate::engine::JournalHandle;
use crate::window::PennaFrontendWindow;
use crate::{grid, i18n, sync};

type ApplyTag = Rc<dyn Fn(String, bool)>;
type ActivatePill = Rc<dyn Fn(PillKind, &str)>;

/// Wrap width used before the dialog is allocated; corrected by a re-render
/// once the real content width is known.
const DEFAULT_WRAP_WIDTH: i32 = 380;

#[derive(Clone, Copy)]
enum PillKind {
    /// A name in the search box that no existing tag matches (dashed, plus).
    New,
    /// A tag already on the open note (solid, cross).
    Attached,
    /// A tag on other notes but not this one (outline, plus).
    Existing,
}

/// Builds one pill widget (label + state button, clickable). `widget_name`
/// lets callers find it again for focus restoration and keyboard dispatch.
fn build_pill(
    widget_name: &str,
    label_text: &str,
    kind: PillKind,
    on_activate: Rc<dyn Fn() + 'static>,
) -> gtk::Box {
    let pill = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    pill.add_css_class(match kind {
        PillKind::Attached => "tag-pill-attached",
        PillKind::Existing => "tag-pill-existing",
        PillKind::New => "tag-pill-new",
    });
    // Never stretch: hug exactly [label][button].
    pill.set_hexpand(false);
    pill.set_halign(gtk::Align::Start);
    pill.set_valign(gtk::Align::Start);
    pill.set_widget_name(widget_name);
    pill.set_focusable(true);

    let label = gtk::Label::new(Some(label_text));
    label.set_ellipsize(pango::EllipsizeMode::End);
    label.set_single_line_mode(true);
    label.add_css_class("tag-pill-label");
    pill.append(&label);

    let (icon, tooltip) = match kind {
        PillKind::Attached => ("window-close-symbolic", i18n::tag_remove_tooltip()),
        _ => ("list-add-symbolic", i18n::tag_add_tooltip()),
    };
    let button = gtk::Button::new();
    button.set_icon_name(icon);
    button.add_css_class("flat");
    button.add_css_class("circular");
    button.set_tooltip_text(Some(tooltip.as_str()));
    let on_activate_for_button = on_activate.clone();
    button.connect_clicked(move |_| on_activate_for_button());
    pill.append(&button);

    // A click anywhere on the pill body toggles it too.
    let on_activate_for_body = on_activate.clone();
    let click = gtk::GestureClick::new();
    click.connect_released(move |_, _, _, _| on_activate_for_body());
    pill.add_controller(click);

    // Enter / Space on a focused pill activates it.
    let on_activate_for_keys = on_activate;
    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(move |_, keyval, _, _| {
        if matches!(
            keyval,
            gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::ISO_Enter | gdk::Key::space | gdk::Key::KP_Space
        ) {
            on_activate_for_keys();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    pill.add_controller(keys);

    pill
}

/// Opens the tags dialog for the currently open entry.
pub fn show(window: &PennaFrontendWindow, handle: JournalHandle, entry_id: String) {
    let imp = window.imp();

    let dialog = adw::Dialog::new();
    dialog.set_title(&i18n::tags_title());
    dialog.set_content_width(400);
    dialog.set_content_height(500);

    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.add_css_class("flat");
    header.set_title_widget(Some(&adw::WindowTitle::new(&i18n::tags_title(), "")));
    toolbar.add_top_bar(&header);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    toolbar.set_content(Some(&content));
    dialog.set_child(Some(&toolbar));

    let search_entry = gtk::SearchEntry::new();
    search_entry.set_placeholder_text(Some(&i18n::tags_filter_placeholder()));
    content.append(&search_entry);

    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_hscrollbar_policy(gtk::PolicyType::Never);
    scrolled.set_vscrollbar_policy(gtk::PolicyType::Automatic);
    scrolled.set_propagate_natural_height(true);
    scrolled.set_max_content_height(360);
    scrolled.set_min_content_height(200);

    // Manual wrap container: a vertical column of horizontal rows of pills.
    let rows_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
    rows_box.set_halign(gtk::Align::Fill);
    rows_box.set_valign(gtk::Align::Start);
    rows_box.add_css_class("tag-rows");
    scrolled.set_child(Some(&rows_box));

    let empty_page = gtk::Box::new(gtk::Orientation::Vertical, 6);
    empty_page.set_valign(gtk::Align::Center);
    empty_page.set_halign(gtk::Align::Center);
    let empty_title = gtk::Label::new(Some(&i18n::tags_empty_title()));
    empty_title.add_css_class("title-2");
    let empty_hint = gtk::Label::new(Some(&i18n::tags_empty_hint()));
    empty_hint.add_css_class("dim-label");
    empty_hint.set_wrap(true);
    empty_page.append(&empty_title);
    empty_page.append(&empty_hint);

    let pages = gtk::Stack::new();
    pages.set_vexpand(true);
    pages.add_named(&scrolled, Some("list"));
    pages.add_named(&empty_page, Some("empty"));
    content.append(&pages);

    let available_tags = Rc::new(RefCell::new(sync::list_tags(window, handle)));
    let current_tags = Rc::new(RefCell::new(imp.current_entry_tags.borrow().clone()));

    type RenderFn = Rc<dyn Fn()>;
    let render_handle: Rc<RefCell<Option<RenderFn>>> = Rc::new(RefCell::new(None));

    // apply a tag to (or remove a tag from) the note, then re-render.
    let apply_tag: ApplyTag = {
        let current_tags = current_tags.clone();
        let render_handle = render_handle.clone();
        Rc::new(glib::clone!(
            #[weak(rename_to = window)]
            window,
            #[strong]
            entry_id,
            move |tag, attach: bool| {
                let result = if attach {
                    sync::add_tag(&window, handle, &entry_id, &tag)
                } else {
                    sync::remove_tag(&window, handle, &entry_id, &tag)
                };
                if let Ok(tags) = result {
                    *current_tags.borrow_mut() = tags.clone();
                    *window.imp().current_entry_tags.borrow_mut() = tags;
                    grid::refresh_notes_grid(&window);
                    if let Some(render) = render_handle.borrow().as_ref() {
                        render();
                    }
                }
            }
        ))
    };

    // create a brand-new tag from the search box text.
    let create_from_search: Rc<dyn Fn()> = {
        let search_entry = search_entry.clone();
        let available_tags = available_tags.clone();
        let current_tags = current_tags.clone();
        let render_handle = render_handle.clone();
        Rc::new(glib::clone!(
            #[weak(rename_to = window)]
            window,
            #[strong]
            entry_id,
            move || {
                let tag = search_entry.text().trim().to_string();
                if tag.is_empty() {
                    return;
                }
                if let Ok(tags) = sync::add_tag(&window, handle, &entry_id, &tag) {
                    if !available_tags.borrow().iter().any(|t| t == &tag) {
                        available_tags.borrow_mut().push(tag.clone());
                        available_tags.borrow_mut().sort_unstable();
                    }
                    *current_tags.borrow_mut() = tags.clone();
                    *window.imp().current_entry_tags.borrow_mut() = tags;
                    search_entry.set_text("");
                    grid::refresh_notes_grid(&window);
                    if let Some(render) = render_handle.borrow().as_ref() {
                        render();
                    }
                }
            }
        ))
    };

    // Route a click/activation to the right mutation by kind.
    let on_pill_activate: ActivatePill = {
        let apply_tag = apply_tag.clone();
        let create_from_search = create_from_search.clone();
        Rc::new(move |kind, name| match kind {
            PillKind::New => create_from_search(),
            PillKind::Attached => apply_tag(name.to_string(), false),
            PillKind::Existing => apply_tag(name.to_string(), true),
        })
    };

    let render: RenderFn = {
        let rows_box = rows_box.clone();
        let scrolled = scrolled.clone();
        let pages = pages.clone();
        let search_entry = search_entry.clone();
        let available_tags = available_tags.clone();
        let current_tags = current_tags.clone();
        let on_pill_activate = on_pill_activate.clone();
        Rc::new(move || {
            // Remember the focused pill to restore focus after the rebuild.
            let mut focused_name: Option<String> = None;
            let mut iter = rows_box.first_child();
            while let Some(row) = iter {
                let mut pill = row.first_child();
                while let Some(p) = pill {
                    if p.has_focus() {
                        focused_name = Some(p.widget_name().to_string());
                    }
                    pill = p.next_sibling();
                }
                iter = row.next_sibling();
            }

            let mut it = rows_box.first_child();
            while let Some(child) = it {
                let next = child.next_sibling();
                rows_box.remove(&child);
                it = next;
            }

            let query = search_entry.text().trim().to_string();
            let lower_query = query.to_lowercase();

            let current = current_tags.borrow().clone();
            let available = available_tags.borrow().clone();

            // Order: attached, then create-new, then existing-unselected.
            let mut attached: Vec<String> = current.clone();
            attached.sort_unstable();
            attached.retain(|t| lower_query.is_empty() || t.to_lowercase().contains(&lower_query));

            let mut existing: Vec<String> = available
                .iter()
                .filter(|t| !current.contains(t))
                .cloned()
                .collect();
            existing.retain(|t| lower_query.is_empty() || t.to_lowercase().contains(&lower_query));
            existing.sort_unstable();

            let mut known_lower: std::collections::HashSet<String> =
                current.iter().map(|t| t.to_lowercase()).collect();
            known_lower.extend(existing.iter().map(|t| t.to_lowercase()));
            let show_new = !query.is_empty() && !known_lower.contains(&lower_query);

            // Materialize the ordered pill specs.
            let order: Vec<(String, PillKind)> = attached
                .iter()
                .map(|t| (t.clone(), PillKind::Attached))
                .chain(show_new.then(|| (query.clone(), PillKind::New)))
                .chain(existing.iter().map(|t| (t.clone(), PillKind::Existing)))
                .collect();

            // Build the pills, then pack them into horizontal rows by measured
            // width so they hug content and wrap, never one-per-line spanning
            // the full width.
            let avail = {
                let w = scrolled.width();
                if w <= 0 { DEFAULT_WRAP_WIDTH } else { w }
            };

            let mut row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            row.set_halign(gtk::Align::Start);
            row.set_valign(gtk::Align::Start);
            row.set_hexpand(false);
            let mut row_w = 0i32;

            for (name_arg, kind) in &order {
                let on = on_pill_activate.clone();
                let k = *kind;
                let name = name_arg.clone();
                let handler_name = name.clone();
                let pill = build_pill(
                    &name,
                    &name,
                    k,
                    Rc::new(move || on(k, &handler_name)),
                );
                let w = pill.measure(gtk::Orientation::Horizontal, -1).1;
                if row_w > 0 && row_w + w + 8 > avail {
                    rows_box.append(&row);
                    row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                    row.set_halign(gtk::Align::Start);
                    row.set_valign(gtk::Align::Start);
                    row.set_hexpand(false);
                    row_w = 0;
                }
                row.append(&pill);
                row_w += w + 8;
            }
            rows_box.append(&row);

            let no_tags_at_all = available.is_empty();
            pages.set_visible_child_name(if no_tags_at_all && query.is_empty() {
                "empty"
            } else {
                "list"
            });

            if let Some(name) = focused_name {
                let mut iter = rows_box.first_child();
                while let Some(row) = iter {
                    let mut pill = row.first_child();
                    while let Some(p) = pill {
                        if p.widget_name() == name {
                            p.grab_focus();
                            return;
                        }
                        pill = p.next_sibling();
                    }
                    iter = row.next_sibling();
                }
            }
        })
    };

    *render_handle.borrow_mut() = Some(render.clone());

    // Arrow-key navigation between pills, plus Enter on a focused pill, via
    // the row boxes.
    let pills_keys = gtk::EventControllerKey::new();
    pills_keys.connect_key_pressed(glib::clone!(
        #[strong]
        rows_box,
        move |_, keyval, _, _| {
            let diri = match keyval {
                gdk::Key::Right | gdk::Key::KP_Right => 1i32,
                gdk::Key::Left | gdk::Key::KP_Left => -1i32,
                gdk::Key::Down | gdk::Key::KP_Down => 1i32,
                gdk::Key::Up | gdk::Key::KP_Up => -1i32,
                _ => return glib::Propagation::Proceed,
            };
            // Gather all pills in a flat list and move focus by `diri`.
            let mut pills: Vec<gtk::Widget> = Vec::new();
            let mut iter = rows_box.first_child();
            while let Some(row) = iter {
                let mut p = row.first_child();
                while let Some(w) = p {
                    pills.push(w.clone());
                    p = w.next_sibling();
                }
                iter = row.next_sibling();
            }
            if pills.is_empty() {
                return glib::Propagation::Stop;
            }
            let current = pills.iter().position(|w| w.has_focus());
            let next = match current {
                Some(i) => ((i as i32 + diri).rem_euclid(pills.len() as i32)) as usize,
                None => 0,
            };
            pills[next].grab_focus();
            glib::Propagation::Stop
        }
    ));
    rows_box.add_controller(pills_keys);

    let search_keys = gtk::EventControllerKey::new();
    search_keys.connect_key_pressed(glib::clone!(
        #[strong]
        rows_box,
        #[strong]
        search_entry,
        #[strong]
        available_tags,
        #[strong]
        on_pill_activate,
        #[strong]
        create_from_search,
        #[strong]
        dialog,
        move |_, keyval, _, _| {
            if keyval == gdk::Key::Escape {
                dialog.close();
                return glib::Propagation::Stop;
            }
            if matches!(keyval, gdk::Key::Down | gdk::Key::KP_Down) {
                if let Some(first_pill) = rows_box
                    .first_child()
                    .and_then(|row| row.first_child())
                {
                    first_pill.grab_focus();
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
                    .find(|t| t.to_lowercase() == query.to_lowercase())
                    .cloned();
                match exact {
                    Some(tag) => on_pill_activate(PillKind::Existing, &tag),
                    None => create_from_search(),
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        }
    ));
    search_entry.add_controller(search_keys);

    search_entry.connect_search_changed(glib::clone!(
        #[strong]
        render,
        move |_| {
            render();
        }
    ));

    search_entry.connect_activate(glib::clone!(
        #[strong]
        create_from_search,
        move |_| {
            create_from_search();
        }
    ));

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

    render();
    dialog.present(Some(window));
    search_entry.grab_focus();

    // The wrap width is only known after the dialog is allocated; re-render
    // once so the pills pack to the real content width.
    let render = render.clone();
    glib::idle_add_local_once(move || render());
}
