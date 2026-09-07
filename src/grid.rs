//! Notes-grid construction, search filtering, and selection bookkeeping.
//!
//! The window owns the template widgets (`notes_flowbox`,
//! `notes_search_entry`, …) plus the selection state
//! (`grid_selected_entry_id`, view flags); everything in this module is
//! plain logic layered on top of them.

use gtk::glib;
use gtk::glib::prelude::*;
use gtk::pango;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use crate::engine::{EntrySummary, JournalStatus};
use crate::format;
use crate::i18n;
use crate::sync;
use crate::window::PennaFrontendWindow;

const TAG_ROW_SPACING: i32 = 6;
const TAG_ROW_HORIZONTAL_SLACK: i32 = 24;

struct GridData {
    entries: Vec<EntrySummary>,
    conflicted_ids: Vec<String>,
    contents: Vec<String>,
    status: Option<JournalStatus>,
}

pub fn refresh_notes_grid(window: &PennaFrontendWindow) {
    let imp = window.imp();
    let Some(handle) = *imp.current_handle.borrow() else {
        return;
    };

    let query = imp.notes_search_entry.text().trim().to_lowercase();

    // The journal reads (list, per-note content, conflicted ids, status) run
    // off the main thread so refreshing the grid never freezes the UI. Only
    // widget building stays on the main thread, once the data lands.
    sync::offload(
        window,
        move |engine| {
            let engine = engine.lock().unwrap();
            let entries = engine.list_entries(handle);
            let conflicted_ids = engine.conflicted_entry_ids(handle);
            let contents = entries
                .iter()
                .map(|entry| {
                    engine
                        .get_entry(handle, &entry.entry_id)
                        .map(|record| record.content)
                        .unwrap_or_default()
                })
                .collect();
            let status = engine.journal_status(handle);
            GridData {
                entries,
                conflicted_ids,
                contents,
                status,
            }
        },
        glib::clone!(
            #[weak(rename_to = window)]
            window,
            move |data: GridData| {
                build_grid_rows(&window, data, &query)
            }
        ),
    );
}

fn build_grid_rows(window: &PennaFrontendWindow, data: GridData, query: &str) {
    let imp = window.imp();

    while let Some(child) = imp.notes_flowbox.first_child() {
        imp.notes_flowbox.remove(&child);
    }

    let mut first_visible_button: Option<gtk::Button> = None;

    for (entry, content) in data.entries.iter().zip(data.contents.iter()) {
        if !entry_matches_query(&entry.entry_id, content, &entry.tags, query) {
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

        let note_label = gtk::Label::new(Some(&format::format_entry_date(&entry.entry_id)));
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

        let leading_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        leading_box.set_hexpand(true);
        if data.conflicted_ids.iter().any(|id| id == &entry.entry_id) {
            let conflict_icon = gtk::Image::from_icon_name("dialog-warning-symbolic");
            conflict_icon.set_tooltip_text(Some(&i18n::unresolved_sync_conflict()));
            conflict_icon.add_css_class("warning");
            conflict_icon.set_valign(gtk::Align::Center);
            leading_box.append(&conflict_icon);
        }
        leading_box.append(&note_label);
        row_box.set_start_widget(Some(&leading_box));
        if !entry.tags.is_empty() {
            for tag in &entry.tags {
                tags_inner.append(&build_tag_chip(tag));
            }
            if entry.tags.len() > 1 {
                let plus_chip = build_tag_chip(&format!("+{}", entry.tags.len() - 1));
                plus_chip.set_visible(false);
                tags_inner.append(&plus_chip);
            }
            set_widget_data(&button, "penna-note-tag-count", entry.tags.len());
        }
        row_box.set_end_widget(Some(&tags_box));
        button.set_child(Some(&row_box));

        let entry_id = entry.entry_id.clone();
        button.connect_clicked(glib::clone!(
            #[weak(rename_to = window)]
            window,
            #[strong]
            entry_id,
            move |_| {
                select_note(&window, Some(&entry_id));
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
    imp.notes_empty_state.set_visible(data.entries.is_empty());

    // Keep highlighting a stable selection across refreshes; if the
    // previously selected note is gone (deleted, filtered out), fall back
    // to the first visible row so something is always selected.
    let buttons = note_buttons(window);
    let selected_still_visible = imp
        .grid_selected_entry_id
        .borrow()
        .as_deref()
        .is_some_and(|id| buttons.iter().any(|button| button.widget_name() == id));
    if !selected_still_visible {
        *imp.grid_selected_entry_id.borrow_mut() =
            buttons.first().map(|b| b.widget_name().to_string());
    }
    refresh_grid_selection(window);

    if query.is_empty() && *imp.in_notes_grid_view.borrow() {
        if let Some(button) = first_visible_button {
            button.grab_focus();
        }
    }

    glib::idle_add_local_once(glib::clone!(
        #[weak(rename_to = window)]
        window,
        move || {
            update_tag_overflow(&window);
        }
    ));

    if let Some(status) = &data.status {
        imp.sync_status_label.set_label(&status_details(status));
    }
}

/// One-line status-bar summary of the journal (branch, head, dirty flag,
/// entry count, and any in-progress merge).
pub(crate) fn status_details(status: &JournalStatus) -> String {
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
    details
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

fn note_buttons(window: &PennaFrontendWindow) -> Vec<gtk::Button> {
    let imp = window.imp();
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

pub fn selected_note_button(window: &PennaFrontendWindow) -> Option<gtk::Button> {
    let selected = window.imp().grid_selected_entry_id.borrow().clone()?;
    note_buttons(window)
        .into_iter()
        .find(|button| button.widget_name() == selected.as_str())
}

/// Marks `entry_id` as the grid's current selection and paints the
/// persistent highlight. Selection is independent of GTK keyboard-focus
/// visibility, so it is visible before any arrow key is pressed.
fn select_note(window: &PennaFrontendWindow, entry_id: Option<&str>) {
    *window.imp().grid_selected_entry_id.borrow_mut() = entry_id.map(str::to_string);
    refresh_grid_selection(window);
}

fn refresh_grid_selection(window: &PennaFrontendWindow) {
    let selected = window.imp().grid_selected_entry_id.borrow().clone();
    for button in note_buttons(window) {
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

fn notes_grid_column_count(
    window: &PennaFrontendWindow,
    total_buttons: usize,
    buttons: &[gtk::Button],
) -> usize {
    if total_buttons <= 1 {
        return 1;
    }

    let imp = window.imp();
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

pub fn move_note_focus(window: &PennaFrontendWindow, direction: &str) -> bool {
    let buttons = note_buttons(window);
    if buttons.is_empty() {
        return false;
    }

    let selected_id = window.imp().grid_selected_entry_id.borrow().clone();
    let current = buttons
        .iter()
        .position(|button| selected_id.as_deref() == Some(button.widget_name().as_str()))
        .unwrap_or(0);
    let cols = notes_grid_column_count(window, buttons.len(), &buttons);
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
            select_note(window, Some(&button.widget_name()));
            button.grab_focus();
            return true;
        }
    }

    false
}

pub fn start_notes_search(window: &PennaFrontendWindow, ch: char) {
    let imp = window.imp();
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

pub fn update_notes_search_reveal(window: &PennaFrontendWindow) {
    let imp = window.imp();
    let reveal = *imp.in_notes_grid_view.borrow()
        && !*imp.in_editor_view.borrow()
        && !imp.notes_search_entry.text().trim().is_empty();
    imp.notes_search_revealer.set_reveal_child(reveal);
}

pub fn connect_tag_overflow(window: &PennaFrontendWindow) {
    let flowbox: &gtk::FlowBox = &window.imp().notes_flowbox;
    flowbox.connect_map(glib::clone!(
        #[weak(rename_to = window)]
        window,
        move |_| {
            glib::idle_add_local_once(glib::clone!(
                #[weak(rename_to = window)]
                window,
                move || {
                    update_tag_overflow(&window);
                }
            ));
        }
    ));

    let weak = window.downgrade();
    glib::timeout_add_local(std::time::Duration::from_millis(150), move || {
        let Some(window) = weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        let width = window.width();
        let imp = window.imp();
        if width != imp.tag_overflow_last_width.get() {
            imp.tag_overflow_last_width.set(width);
            if *imp.in_notes_grid_view.borrow() {
                update_tag_overflow(&window);
            }
        }
        glib::ControlFlow::Continue
    });
}

fn update_tag_overflow(window: &PennaFrontendWindow) {
    for button in note_buttons(window) {
        update_row_tag_overflow(&button);
    }
}

fn update_row_tag_overflow(button: &gtk::Button) {
    let Some(count) = widget_data::<gtk::Button, usize>(button, "penna-note-tag-count") else {
        return;
    };
    if count == 0 {
        return;
    }

    let Some(row_box) = button
        .child()
        .and_then(|widget| widget.downcast::<gtk::CenterBox>().ok())
    else {
        return;
    };
    let Some(leading_box) = row_box
        .start_widget()
        .and_then(|widget| widget.downcast::<gtk::Box>().ok())
    else {
        return;
    };
    let Some(tags_box) = row_box
        .end_widget()
        .and_then(|widget| widget.downcast::<gtk::Box>().ok())
    else {
        return;
    };
    let Some(tags_inner) = tags_box
        .first_child()
        .and_then(|widget| widget.next_sibling())
        .and_then(|widget| widget.downcast::<gtk::Box>().ok())
    else {
        return;
    };

    let row_width = row_box.width();
    if row_width <= 0 {
        return;
    }

    let tag_chips = row_children(&tags_inner);
    let tag_chip_count = tag_chips.len().min(count);
    if tag_chip_count == 0 {
        return;
    }

    let mut widths = widget_data::<gtk::Button, Vec<i32>>(button, "penna-note-tag-widths")
        .filter(|widths| widths.len() == count);
    if widths.is_none() {
        let measured: Vec<i32> = tag_chips
            .iter()
            .take(count)
            .map(|chip| chip.measure(gtk::Orientation::Horizontal, -1).1.max(0))
            .collect();
        if measured.iter().all(|width| *width > 0) {
            set_widget_data(button, "penna-note-tag-widths", measured.clone());
            widths = Some(measured);
        }
    }
    let Some(widths) = widths else {
        return;
    };

    let leading_width = leading_box
        .measure(gtk::Orientation::Horizontal, -1)
        .1
        .max(0);
    let available = (row_width as i64 - leading_width as i64 - TAG_ROW_HORIZONTAL_SLACK as i64)
        .max(0);

    let all_width: i64 = widths
        .iter()
        .take(count)
        .map(|width| *width as i64)
        .sum::<i64>()
        + TAG_ROW_SPACING as i64 * (count - 1) as i64;

    let plus_chip = if count > 1 { tag_chips.last() } else { None };

    let visible_count = if all_width <= available {
        count
    } else {
        match plus_chip {
            Some(plus_chip) => {
                let plus_widths = plus_widths_for_count(button, plus_chip, count);
                (1..count).rev().find(|&visible| {
                    let hidden = count - visible;
                    let plus_width =
                        plus_widths.get(hidden - 1).copied().unwrap_or(0) as i64;
                    let tags_width: i64 = widths
                        .iter()
                        .take(visible)
                        .map(|width| *width as i64)
                        .sum::<i64>()
                        + if visible > 1 {
                            TAG_ROW_SPACING as i64 * (visible - 1) as i64
                        } else {
                            0
                        };
                    tags_width + TAG_ROW_SPACING as i64 + plus_width <= available
                })
                .unwrap_or(0)
            }
            None => 1,
        }
    };

    for (idx, chip) in tag_chips.iter().take(count).enumerate() {
        let visible = idx < visible_count;
        if chip.is_visible() != visible {
            chip.set_visible(visible);
        }
    }

    if let Some(plus_chip) = plus_chip {
        let visible = visible_count < count;
        if visible {
            set_tag_chip_label(plus_chip, &format!("+{}", count - visible_count));
        }
        if plus_chip.is_visible() != visible {
            plus_chip.set_visible(visible);
        }
    }
}

fn plus_widths_for_count(
    button: &gtk::Button,
    plus_chip: &gtk::Box,
    count: usize,
) -> Vec<i32> {
    if let Some(widths) = widget_data::<gtk::Button, Vec<i32>>(button, "penna-note-plus-widths")
        .filter(|widths| widths.len() == count - 1)
    {
        return widths;
    }

    let original_label = plus_chip
        .first_child()
        .and_then(|widget| widget.downcast::<gtk::Label>().ok())
        .map(|label| label.text().to_string());
    let was_visible = plus_chip.is_visible();
    plus_chip.set_visible(true);

    let mut widths = Vec::with_capacity(count.saturating_sub(1));
    for hidden in 1..count {
        set_tag_chip_label(plus_chip, &format!("+{hidden}"));
        widths.push(
            plus_chip
                .measure(gtk::Orientation::Horizontal, -1)
                .1
                .max(0),
        );
    }

    if let Some(label) = original_label {
        set_tag_chip_label(plus_chip, &label);
    }
    plus_chip.set_visible(was_visible);

    set_widget_data(button, "penna-note-plus-widths", widths.clone());
    widths
}

fn row_children(widget: &gtk::Box) -> Vec<gtk::Box> {
    let mut out = Vec::new();
    let mut child = widget.first_child();

    while let Some(current) = child {
        if let Some(box_widget) = current.downcast_ref::<gtk::Box>() {
            out.push(box_widget.clone());
        }
        child = current.next_sibling();
    }

    out
}

fn set_tag_chip_label(chip: &gtk::Box, label: &str) {
    if let Some(text) = chip
        .first_child()
        .and_then(|widget| widget.downcast::<gtk::Label>().ok())
    {
        text.set_label(label);
    }
}

fn set_widget_data<T: IsA<gtk::Widget> + 'static, V: 'static>(
    object: &T,
    key: &str,
    value: V,
) {
    unsafe { object.set_data(key, value) };
}

fn widget_data<T: IsA<gtk::Widget> + 'static, V: Clone + 'static>(
    object: &T,
    key: &str,
) -> Option<V> {
    unsafe { object.data::<V>(key) }.map(|pointer| unsafe { pointer.as_ref().clone() })
}

pub(crate) fn build_tag_chip(tag: &str) -> gtk::Box {
    let chip = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    chip.add_css_class("tag-chip");

    let label = gtk::Label::new(Some(tag));
    label.add_css_class("caption");
    chip.append(&label);

    chip
}
