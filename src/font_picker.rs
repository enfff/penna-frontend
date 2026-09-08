//! Keyboard-friendly custom font family picker.
//!
//! Replaces `gtk::FontDialog::choose_family`: on gtk4 4.22 the keyboard
//! activation path of that dialog hands the finish callback a non-FontFamily
//! object (pango_font_family_get_name CRITICAL + SIGSEGV), so we present our
//! own searchable list where Enter activates a real `pango::FontFamily`.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{glib, pango};

pub fn show_family_picker<F>(parent: &impl IsA<gtk::Widget>, on_select: F)
where
    F: Fn(&str) + 'static,
{
    let query: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));

    let font_map = parent
        .pango_context()
        .font_map()
        .expect("widget pango context has no font map");
    let filter = gtk::CustomFilter::new({
        let query = query.clone();
        move |obj| {
            let Some(family) = obj.downcast_ref::<pango::FontFamily>() else {
                return false;
            };
            let query = query.borrow();
            query.is_empty()
                || family.name().to_lowercase().contains(&query.to_lowercase())
        }
    });
    let filter_model = gtk::FilterListModel::new(Some(font_map), Some(filter.clone()));
    let sorter = gtk::CustomSorter::new(|a, b| {
        let name = |obj: &glib::Object| {
            obj.downcast_ref::<pango::FontFamily>()
                .map(|family| family.name().to_lowercase())
                .unwrap_or_default()
        };
        name(a).cmp(&name(b)).into()
    });
    let sort_model = gtk::SortListModel::new(Some(filter_model), Some(sorter));
    let selection = gtk::SingleSelection::new(Some(sort_model));
    selection.set_autoselect(false);
    selection.set_can_unselect(true);

    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let label = gtk::Label::new(None);
        label.set_halign(gtk::Align::Start);
        label.set_margin_top(6);
        label.set_margin_bottom(6);
        label.set_margin_start(8);
        label.set_margin_end(8);
        item.set_child(Some(&label));
    });
    factory.connect_bind(|_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(family) = item.item().and_downcast::<pango::FontFamily>() else {
            return;
        };
        if let Some(label) = item.child().and_downcast::<gtk::Label>() {
            label.set_text(&family.name());
        }
    });

    let list = gtk::ListView::new(Some(selection.clone()), Some(factory));
    list.set_single_click_activate(true);

    let dialog = adw::Dialog::new();
    dialog.set_title("Select a Font Family");
    dialog.set_content_width(440);
    dialog.set_content_height(560);

    let choose = {
        let dialog = dialog.clone();
        Rc::new(move |family_name: &str| {
            on_select(family_name);
            dialog.force_close();
        })
    };

    // Enter on a focused row activates it.
    list.connect_activate({
        let selection = selection.clone();
        let choose = choose.clone();
        move |_, position| {
            if let Some(family) = selection
                .item(position)
                .and_downcast::<pango::FontFamily>()
            {
                choose(&family.name());
            }
        }
    });

    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Search font families"));
    search.connect_search_changed({
        let query = query.clone();
        let filter = filter.clone();
        move |entry| {
            query.replace(entry.text().to_string());
            filter.changed(gtk::FilterChange::Different);
        }
    });

    // Enter in the search field picks the first visible family.
    search.connect_activate({
        let selection = selection.clone();
        let choose = choose;
        move |_| {
            if selection.n_items() > 0 {
                if let Some(family) = selection.item(0).and_downcast::<pango::FontFamily>() {
                    choose(&family.name());
                }
            }
        }
    });

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&search));

    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_child(Some(&list));
    scrolled.set_vexpand(true);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&scrolled));

    dialog.set_child(Some(&toolbar));
    dialog.present(Some(parent));
}
