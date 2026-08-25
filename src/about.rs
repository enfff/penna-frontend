//! About dialog construction.

use gettextrs::gettext;

use adw::prelude::*;

use crate::application::PennaFrontendApplication;
use crate::config::VERSION;

pub fn show_about(app: &PennaFrontendApplication) {
    let window = app.active_window().unwrap();
    let about = adw::AboutDialog::builder()
        .application_name("Diary")
        .application_icon("io.github.enfff.Diary")
        .developer_name("Francesco P. Carmone")
        .version(VERSION)
        .developers(vec!["Francesco P. Carmone"])
        .website("https://github.com/enfff/penna-frontend")
        .issue_url("https://github.com/enfff/penna-frontend/issues")
        // Translators: Replace "translator-credits" with your name/username, and optionally an email or URL.
        .translator_credits(gettext("translator-credits"))
        .copyright("© 2026 Francesco P. Carmone")
        .build();

    about.present(Some(&window));
}
