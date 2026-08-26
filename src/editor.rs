//! Editor chrome: css provider, text-tag setup, viewer-mode toggling, and
//! font-size handling.
//!
//! The window owns the template widgets (`editor_view`,
//! `viewer_mode_button`) plus the editor state cells (`editor_css_provider`,
//! `editor_font_size_pt`, `editor_viewer_mode`); everything in this module
//! operates on those through the window reference.

use gtk::gdk;
use gtk::pango;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use crate::settings;
use crate::window::PennaFrontendWindow;

pub const EDITOR_FONT_SIZE_DEFAULT_PT: i32 = 14;
pub const EDITOR_FONT_SIZE_MIN_PT: i32 = 10;
pub const EDITOR_FONT_SIZE_MAX_PT: i32 = 28;

pub const TAG_HEADING_1: &str = "md-heading-1";
pub const TAG_HEADING_2: &str = "md-heading-2";
pub const TAG_HEADING_3: &str = "md-heading-3";
pub const TAG_HEADING_4: &str = "md-heading-4";
pub const TAG_BLOCKQUOTE: &str = "md-blockquote";
pub const TAG_CODE: &str = "md-code";
pub const TAG_CODE_BLOCK: &str = "md-code-block";
pub const TAG_BOLD: &str = "md-bold";
pub const TAG_ITALIC: &str = "md-italic";
pub const TAG_STRIKETHROUGH: &str = "md-strikethrough";
pub const TAG_SYNTAX: &str = "md-syntax";
pub const TAG_LIST_MARKER: &str = "md-list-marker";
pub const TAG_LIST_ITEM: &str = "md-list-item";
pub const TAG_LINK: &str = "md-link";
pub const TAG_CHECKED: &str = "md-checked";
pub const TAG_RULE: &str = "md-rule";
pub const TAG_CONFLICT_CURRENT: &str = "conflict-current";
pub const TAG_CONFLICT_INCOMING: &str = "conflict-incoming";
pub const TAG_CONFLICT_MARKER: &str = "conflict-marker";

pub fn editor_font_size_pt(window: &PennaFrontendWindow) -> i32 {
    *window.imp().editor_font_size_pt.borrow()
}

pub fn set_editor_font_size_pt(window: &PennaFrontendWindow, size_pt: i32) {
    let next_size = size_pt.clamp(EDITOR_FONT_SIZE_MIN_PT, EDITOR_FONT_SIZE_MAX_PT);

    if next_size == *window.imp().editor_font_size_pt.borrow() {
        return;
    }

    *window.imp().editor_font_size_pt.borrow_mut() = next_size;
    apply_editor_css(window);
}

pub fn adjust_editor_zoom(window: &PennaFrontendWindow, delta: i32) {
    let imp = window.imp();
    let next_size = (*imp.editor_font_size_pt.borrow() + delta)
        .clamp(EDITOR_FONT_SIZE_MIN_PT, EDITOR_FONT_SIZE_MAX_PT);

    if next_size == *imp.editor_font_size_pt.borrow() {
        return;
    }

    set_editor_font_size_pt(window, next_size);
}

pub fn load_editor_preferences(window: &PennaFrontendWindow) {
    let viewer_mode = settings::get_bool(settings::SETTINGS_EDITOR_VIEWER_MODE_KEY);
    *window.imp().editor_viewer_mode.borrow_mut() = viewer_mode;
    apply_editor_mode(window);
}

pub fn toggle_viewer_mode(window: &PennaFrontendWindow) {
    let imp = window.imp();
    let next = !*imp.editor_viewer_mode.borrow();
    *imp.editor_viewer_mode.borrow_mut() = next;

    let _ = settings::set_bool(settings::SETTINGS_EDITOR_VIEWER_MODE_KEY, next);

    apply_editor_mode(window);
}

pub fn apply_editor_mode(window: &PennaFrontendWindow) {
    let imp = window.imp();
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

pub fn setup_editor_css(window: &PennaFrontendWindow) {
    let provider = gtk::CssProvider::new();
    *window.imp().editor_css_provider.borrow_mut() = Some(provider.clone());
    apply_editor_css(window);

    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

pub fn apply_editor_css(window: &PennaFrontendWindow) {
    let imp = window.imp();
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
                background: none;\
            }}\
            /* All row shading (hover, press, selection) lives solely on\
             * the flowboxchild wrapper as one rectangle with state-\
             * dependent intensity; painting any tint on the button too\
             * stacks two visible layers. */\
            flowbox.notes-grid > flowboxchild {{\
                border-radius: 10px;\
            }}\
            flowbox.notes-grid > flowboxchild:hover {{\
                background-color: alpha(currentColor, 0.07);\
            }}\
            flowbox.notes-grid > flowboxchild:active {{\
                background-color: alpha(currentColor, 0.12);\
            }}\
            flowbox.notes-grid > flowboxchild.note-current {{\
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

pub fn setup_editor_tags(window: &PennaFrontendWindow) {
    let buffer = window.imp().editor_view.buffer();
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
