//! Notes-grid construction, search filtering, and selection bookkeeping.
//!
//! The window owns the template widgets (`notes_flowbox`,
//! `notes_search_entry`, …) plus the selection state
//! (`grid_selected_entry_id`, view flags); everything in this module is
//! plain logic layered on top of them.

use gtk::glib;
use gtk::pango;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use crate::format;
use crate::i18n;
use crate::sync;
use crate::window::PennaFrontendWindow;

const NOTE_ROW_TAGS_MAX_CHARS: usize = 28;

pub fn refresh_notes_grid(window: &PennaFrontendWindow) {
    let imp = window.imp();
    let Some(handle) = *imp.current_handle.borrow() else {
        return;
    };

    let query = imp.notes_search_entry.text().trim().to_lowercase();

    while let Some(child) = imp.notes_flowbox.first_child() {
        imp.notes_flowbox.remove(&child);
    }

    let entries = sync::list_entries(window, handle);

    // Notes still conflicted mid-merge get a warning badge so unresolved
    // sync state is visible without opening each note.
    let conflicted_ids = sync::conflicted_entry_ids(window, handle);

    let mut first_visible_button: Option<gtk::Button> = None;

    for entry in &entries {
        let content = sync::get_entry(window, handle, &entry.entry_id)
            .map(|item| item.content)
            .unwrap_or_default();

        if !entry_matches_query(&entry.entry_id, &content, &entry.tags, &query) {
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
        if conflicted_ids.iter().any(|id| id == &entry.entry_id) {
            let conflict_icon = gtk::Image::from_icon_name("dialog-warning-symbolic");
            conflict_icon.set_tooltip_text(Some(&i18n::unresolved_sync_conflict()));
            conflict_icon.add_css_class("warning");
            conflict_icon.set_valign(gtk::Align::Center);
            leading_box.append(&conflict_icon);
        }
        leading_box.append(&note_label);
        row_box.set_start_widget(Some(&leading_box));
        if !entry.tags.is_empty() {
            for tag in visible_tags_for_row(&entry.tags) {
                tags_inner.append(&build_tag_chip(tag));
            }
            let hidden_tags = hidden_tag_count_for_row(&entry.tags);
            if hidden_tags > 0 {
                tags_inner.append(&build_tag_chip(&format!("+{hidden_tags}")));
            }
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
    imp.notes_empty_state.set_visible(entries.is_empty());

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

    if let Some(status) = sync::journal_status(window, handle) {
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
    tags.len().saturating_sub(visible_tags_for_row(tags).len())
}

pub(crate) fn build_tag_chip(tag: &str) -> gtk::Box {
    let chip = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    chip.add_css_class("tag-chip");

    let label = gtk::Label::new(Some(tag));
    label.add_css_class("caption");
    chip.append(&label);

    chip
}
