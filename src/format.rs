//! Entry date/time formatting.
//!
//! Everything here turns an entry id (or the persisted datetime-format
//! setting) into display text; no GTK state involved.

use chrono::{format::Item, format::StrftimeItems, NaiveDateTime};

use crate::settings;

pub const ENTRY_DATETIME_FORMAT_DEFAULT: &str = "%Y-%m-%d";

pub struct EntryTimestamp {
    pub value: NaiveDateTime,
}

pub fn format_entry_date(entry_id: &str) -> String {
    let Some(timestamp) = parse_entry_timestamp(entry_id) else {
        return entry_id.to_string();
    };

    let fmt = effective_entry_datetime_format();
    // `write_to` returns a Result instead of panicking: some valid format
    // items (e.g. `%z`, timezone offset) parse fine but cannot be rendered
    // for a `NaiveDateTime`, making chrono's `Display` return an error.
    // Fall back to the default format so a bad persisted setting degrades
    // to a normal date instead of crashing startup.
    let mut buffer = String::new();
    if timestamp.value.format(&fmt).write_to(&mut buffer).is_err() {
        buffer.clear();
        let _ = timestamp
            .value
            .format(ENTRY_DATETIME_FORMAT_DEFAULT)
            .write_to(&mut buffer);
    }
    if buffer.is_empty() {
        return entry_id.to_string();
    }
    buffer
}

fn effective_entry_datetime_format() -> String {
    let raw = settings::get_str(settings::SETTINGS_ENTRY_DATETIME_FORMAT_KEY);
    let candidate = if raw.trim().is_empty() {
        ENTRY_DATETIME_FORMAT_DEFAULT
    } else {
        raw.trim()
    };

    if is_valid_chrono_format(candidate) {
        candidate.to_string()
    } else {
        ENTRY_DATETIME_FORMAT_DEFAULT.to_string()
    }
}

fn is_valid_chrono_format(format: &str) -> bool {
    !StrftimeItems::new(format).any(|item| matches!(item, Item::Error))
}

pub fn parse_entry_timestamp(entry_id: &str) -> Option<EntryTimestamp> {
    let stem = entry_id.strip_suffix(".md")?;
    if stem.len() != 12 || !stem.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }

    let timestamp = NaiveDateTime::parse_from_str(stem, "%Y%m%d%H%M").ok()?;
    Some(EntryTimestamp { value: timestamp })
}

#[cfg(test)]
mod entry_id_tests {
    use super::*;

    const ENTRY_ID_STEM_FORMAT: &str = "%Y%m%d%H%M";

    fn format_id(timestamp: NaiveDateTime) -> String {
        format!("{}.md", timestamp.format(ENTRY_ID_STEM_FORMAT))
    }

    #[test]
    fn round_trip_parse_format_parse() {
        for id in [
            "202608241542.md",
            "202402291230.md",
            "202501010000.md",
            "199912312359.md",
        ] {
            let first =
                parse_entry_timestamp(id).unwrap_or_else(|| panic!("{id} should parse"));
            let formatted = format_id(first.value);
            assert_eq!(formatted, id, "formatting should reproduce the id");

            let second = parse_entry_timestamp(&formatted)
                .unwrap_or_else(|| panic!("canonical {formatted} should re-parse"));
            assert_eq!(second.value, first.value, "re-parsing should be lossless");
        }
    }

    #[test]
    fn round_trip_from_arbitrary_timestamp() {
        let timestamp =
            NaiveDateTime::parse_from_str("2017-05-03T07:09", "%Y-%m-%dT%H:%M").unwrap();
        let id = format_id(timestamp);
        assert_eq!(id, "201705030709.md");

        let parsed = parse_entry_timestamp(&id)
            .unwrap_or_else(|| panic!("{id} should parse"));
        assert_eq!(parsed.value, timestamp);
        assert_eq!(format_id(parsed.value), id);
    }

    #[test]
    fn rejects_wrong_length() {
        assert!(parse_entry_timestamp("20260824154.md").is_none());
        assert!(parse_entry_timestamp("2026082415429.md").is_none());
        assert!(parse_entry_timestamp(".md").is_none());
        assert!(parse_entry_timestamp("").is_none());
    }

    #[test]
    fn rejects_non_digits() {
        assert!(parse_entry_timestamp("2026O8241542.md").is_none());
        assert!(parse_entry_timestamp("2026082A1542.md").is_none());
        assert!(parse_entry_timestamp("abcdefabcdef.md").is_none());
        assert!(parse_entry_timestamp("2026-82415-2.md").is_none());
    }

    #[test]
    fn rejects_wrong_extension() {
        assert!(parse_entry_timestamp("202608241542.txt").is_none());
        assert!(parse_entry_timestamp("202608241542.markdown").is_none());
        assert!(parse_entry_timestamp("202608241542.MD").is_none());
        assert!(parse_entry_timestamp("202608241542").is_none());
        assert!(parse_entry_timestamp("202608241542.md.md").is_none());
    }

    #[test]
    fn rejects_invalid_calendar_values() {
        assert!(parse_entry_timestamp("202602301542.md").is_none());
        assert!(parse_entry_timestamp("202613011542.md").is_none());
        assert!(parse_entry_timestamp("202601012542.md").is_none());
        assert!(parse_entry_timestamp("202402290000.md").is_some());
    }
}
