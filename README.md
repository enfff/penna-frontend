# Diary

A local-first GNOME journal app built with GTK4 and libadwaita. Every entry is
a plain Markdown file in a git repository you own — no accounts, no cloud, no
lock-in. Sync and history come from git itself, with a guided conflict
resolver when branches disagree.

Internal/binary name is `penna-frontend`; the app presents itself as **Diary**.

## Features

- Local-first journaling (plain Markdown files, one note per day)
- Inline Markdown formatting with syntax highlighting and viewer mode
- Full-text search across titles and content
- Git-backed history and sync between machines
- Conflict-safe syncing with a guided resolver

## Building

GNOME Builder opens and builds the Flatpak manifest out of the box:

```sh
flatpak run --command=meson org.gnome.Sdk//50 _build .
ninja -C _build
```

Or with a local toolchain meeting the MSRV (1.92):

```sh
cargo build
```

## Translations

Translations live in `po/` (currently German, French, Italian). Regenerate the
template after changing user-facing strings:

```sh
meson setup _build && ninja -C _build penna-frontend-pot
```

## Code of Conduct

This project follows the
[GNOME Code of Conduct](https://conduct.gnome.org/). By participating you are
expected to uphold it; report issues to the GNOME Code of Conduct Committee via
their [report guide](https://conduct.gnome.org/report-guide/).
