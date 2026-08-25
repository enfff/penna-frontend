# Plan

One task per loop iteration. Tasks are done top-down. The agent marks `- [x]`
only when the gate command passes for that task's work.

## Active

- [x] T13: Proper plural forms — replace the `.replace("{}", …)` pattern for
      count-bearing strings (`unresolved_conflicts`, `conflicts_pending`,
      `sync_conflict_toast_message`) with `gettextrs::ngettext` so translators
      control plural rules per language. Update `src/i18n.rs`, keep all call
      sites compiling, regenerate `po/penna-frontend.pot` via
      `ninja -C <buildir> penna-frontend-pot`, refresh `po/de.po`, `po/fr.po`,
      `po/it.po` with correct plural entries (French/Italian use
      nplurals=2), and confirm `msgfmt --check` passes on all three.
- [x] T14: Translatable desktop metadata — `data/com.github.pennafe.desktop.in`
      currently exposes only the app name. Add translatable `Comment` and
      `GenericName` fields (journal-app wording, English source), regenerate
      the POT, and provide de/fr/it translations for every new msgid.
      Validate the merged desktop file parses (`desktop-file-validate` if
      available).
- [ ] T15: i18n regression guard — add unit tests that pin the i18n module's
      public surface: each helper returns its English msgid under the default
      C locale, placeholders survive `{}` substitution, and
      `sync_conflict_toast_message` keeps its existing singular/plural split.
      Gate must stay green with these tests included in `cargo test`.

## Done

See git history for T1–T12 and the i18n bootstrap (module extraction-safe
helpers, po/{de,fr,it}.po at 100% coverage, LINGUAS + POTFILES wired,
setlocale activation in main.rs).
