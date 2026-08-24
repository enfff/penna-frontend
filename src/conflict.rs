/* conflict.rs
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

#![allow(dead_code)]

const CONFLICT_START_MARKER: &str = "<<<<<<<";
const CONFLICT_SEPARATOR_MARKER: &str = "=======";
const CONFLICT_END_MARKER: &str = ">>>>>>>";

/// A run of entry content classified by the git conflict-marker parser.
///
/// Segment boundaries always fall on line boundaries and every segment keeps
/// its lines verbatim (trailing newlines included), except that the three
/// marker lines of a well-formed block (`<<<<<<<`, `=======`, `>>>>>>>`) are
/// dropped: their position is recoverable from the surrounding segments.
///
/// Line numbers are 0-based indices into the original content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ConflictSegment {
    Normal { start_line: usize, text: String },
    Current { start_line: usize, text: String },
    Incoming { start_line: usize, text: String },
}

impl ConflictSegment {
    pub(crate) fn start_line(&self) -> usize {
        match self {
            Self::Normal { start_line, .. }
            | Self::Current { start_line, .. }
            | Self::Incoming { start_line, .. } => *start_line,
        }
    }

    pub(crate) fn text(&self) -> &str {
        match self {
            Self::Normal { text, .. }
            | Self::Current { text, .. }
            | Self::Incoming { text, .. } => text,
        }
    }

    pub(crate) fn line_count(&self) -> usize {
        self.text().lines().count()
    }

    pub(crate) fn is_conflict(&self) -> bool {
        !matches!(self, Self::Normal { .. })
    }
}

/// Splits entry content into [`ConflictSegment`] runs.
///
/// Markers are recognized only when they start a line and match git's shape:
/// `<<<<<<<` / `>>>>>>>` optionally followed by a space and a label, and a
/// lone `=======`. Anything else — indented copies, longer/shorter runs,
/// mid-line mentions, labels glued to the marker — stays ordinary prose.
///
/// Structurally broken blocks (missing `=======`, unclosed at end of input)
/// cannot come from git, so they are demoted to `Normal` verbatim instead of
/// being misreported as conflicts.
pub(crate) fn split_conflict_segments(content: &str) -> Vec<ConflictSegment> {
    let lines: Vec<&str> = content.split_inclusive('\n').collect();
    let mut segments = Vec::new();
    let mut normal: Vec<&str> = Vec::new();
    let mut normal_start = 0usize;
    let mut i = 0usize;

    while i < lines.len() {
        if !is_start_marker(lines[i]) {
            if normal.is_empty() {
                normal_start = i;
            }
            normal.push(lines[i]);
            i += 1;
            continue;
        }

        let block_start = i;
        i += 1;

        let mut current: Vec<&str> = Vec::new();
        let mut incoming: Vec<&str> = Vec::new();
        let mut in_incoming = false;
        let mut closed = false;

        while i < lines.len() {
            let line = lines[i];
            if !in_incoming && is_separator_marker(line) {
                in_incoming = true;
            } else if is_end_marker(line) {
                closed = in_incoming;
                break;
            } else if in_incoming {
                incoming.push(line);
            } else {
                current.push(line);
            }
            i += 1;
        }

        if closed {
            if !normal.is_empty() {
                segments.push(ConflictSegment::Normal {
                    start_line: normal_start,
                    text: normal.concat(),
                });
                normal.clear();
            }
            let current_start = block_start + 1;
            let incoming_start = current_start + current.len() + 1;
            segments.push(ConflictSegment::Current {
                start_line: current_start,
                text: current.concat(),
            });
            segments.push(ConflictSegment::Incoming {
                start_line: incoming_start,
                text: incoming.concat(),
            });
            i += 1;
        } else {
            let end_exclusive = (i + 1).min(lines.len());
            if normal.is_empty() {
                normal_start = block_start;
            }
            normal.extend_from_slice(&lines[block_start..end_exclusive]);
            i = end_exclusive;
        }
    }

    if !normal.is_empty() {
        segments.push(ConflictSegment::Normal {
            start_line: normal_start,
            text: normal.concat(),
        });
    }

    segments
}

fn is_start_marker(line: &str) -> bool {
    labelled_marker(marker_body(line), CONFLICT_START_MARKER)
}

fn is_end_marker(line: &str) -> bool {
    labelled_marker(marker_body(line), CONFLICT_END_MARKER)
}

fn is_separator_marker(line: &str) -> bool {
    marker_body(line) == CONFLICT_SEPARATOR_MARKER
}

fn marker_body(line: &str) -> &str {
    line.strip_suffix('\n').unwrap_or(line)
}

fn labelled_marker(line: &str, head: &str) -> bool {
    match line.strip_prefix(head) {
        Some(rest) => rest.is_empty() || rest.starts_with(' '),
        None => false,
    }
}

#[cfg(test)]
mod conflict_parser_tests {
    use super::*;

    const BLOCK: &str = "\
<<<<<<< HEAD
mine
=======
theirs
>>>>>>> topic
";

    fn kinds(content: &str) -> Vec<ConflictSegment> {
        split_conflict_segments(content)
    }

    fn assert_normal(seg: &ConflictSegment, start_line: usize, text: &str) {
        match seg {
            ConflictSegment::Normal { .. } => {}
            other => panic!("expected Normal, got {other:?}"),
        }
        assert_eq!(seg.start_line(), start_line);
        assert_eq!(seg.text(), text);
    }

    fn assert_side(seg: &ConflictSegment, start_line: usize, text: &str) {
        assert!(seg.is_conflict(), "expected a conflict side, got {seg:?}");
        assert_eq!(seg.start_line(), start_line);
        assert_eq!(seg.text(), text);
    }

    #[test]
    fn single_block_splits_into_prose_and_sides() {
        let content = format!("intro\n\n{BLOCK}outro\n");
        let segments = kinds(&content);

        assert_eq!(segments.len(), 4);
        assert_normal(&segments[0], 0, "intro\n\n");
        assert_side(&segments[1], 3, "mine\n");
        assert_side(&segments[2], 5, "theirs\n");
        assert_normal(&segments[3], 7, "outro\n");
    }

    #[test]
    fn multiple_blocks_keep_order_even_when_adjacent() {
        let content = format!(
            "{BLOCK}middle\n<<<<<<< one\nz\n=======\nw\n>>>>>>> two\ntail\n"
        );
        let segments = kinds(&content);

        assert_eq!(segments.len(), 6);
        assert_side(&segments[0], 1, "mine\n");
        assert_side(&segments[1], 3, "theirs\n");
        assert_normal(&segments[2], 5, "middle\n");
        assert_side(&segments[3], 7, "z\n");
        assert_side(&segments[4], 9, "w\n");
        assert_normal(&segments[5], 11, "tail\n");
    }

    #[test]
    fn block_at_start_of_file() {
        let content = format!("{BLOCK}after\n");
        let segments = kinds(&content);

        assert_eq!(segments.len(), 3);
        assert_side(&segments[0], 1, "mine\n");
        assert_side(&segments[1], 3, "theirs\n");
        assert_eq!(
            segments.last().map(ConflictSegment::text),
            Some("after\n")
        );
        assert_normal(&segments[2], 5, "after\n");
    }

    #[test]
    fn block_at_end_of_file() {
        let full = format!("before\n{BLOCK}");
        let trimmed = full.trim_end_matches('\n').to_string();
        for content in [full, trimmed] {
            let segments = kinds(&content);

            assert_eq!(segments.len(), 3, "content {content:?}");
            assert_normal(&segments[0], 0, "before\n");
            assert_side(&segments[1], 2, "mine\n");
            assert_side(&segments[2], 4, "theirs\n");
        }
    }

    #[test]
    fn missing_separator_is_plain_prose() {
        let content = "<<<<<<< HEAD\nalpha\n>>>>>>> feature\nbeta\n";
        let segments = kinds(content);

        assert_eq!(segments.len(), 1);
        assert_normal(&segments[0], 0, content);
        assert!(!segments[0].is_conflict());
    }

    #[test]
    fn unclosed_block_at_eof_is_plain_prose() {
        let unclosed_in_incoming =
            "intro\n<<<<<<< HEAD\nalpha\n=======\nbeta";
        let segments = kinds(unclosed_in_incoming);
        assert_eq!(segments.len(), 1);
        assert_normal(&segments[0], 0, unclosed_in_incoming);

        let unclosed_in_current = "intro\n<<<<<<< HEAD\nalpha\nbeta\n";
        let segments = kinds(unclosed_in_current);
        assert_eq!(segments.len(), 1);
        assert_normal(&segments[0], 0, unclosed_in_current);
    }

    #[test]
    fn stray_markers_without_opener_are_plain_prose() {
        let content = "a\n=======\n>>>>>>> nowhere\nb\n";
        let segments = kinds(content);

        assert_eq!(segments.len(), 1);
        assert_normal(&segments[0], 0, content);
    }

    #[test]
    fn marker_like_prose_never_opens_a_block() {
        let cases = [
            "see <<<<<<< demo below\n",
            "  =======\n",
            "\t>>>>>>> note\n",
            "text >>>>>>> more text\n",
            "<<<<<<<HEAD\n",
            ">>>>>>>feature\n",
            "<<<<<<\nx\n",
            "<<<<<<<<<< eight\n",
            "======== seven-plus\n",
        ];
        for case in cases {
            let segments = kinds(case);
            assert_eq!(segments.len(), 1, "case {case:?}");
            assert_normal(&segments[0], 0, case);
        }
    }

    #[test]
    fn marker_positions_follow_the_documented_contract() {
        let content = format!("intro\n{BLOCK}outro\n");
        let segments = kinds(&content);

        let current = &segments[1];
        let incoming = &segments[2];

        assert_eq!(current.start_line() - 1, 1, "header marker line");
        assert_eq!(
            current.start_line() + current.line_count(),
            3,
            "separator marker line"
        );
        assert_eq!(
            incoming.start_line() + incoming.line_count(),
            5,
            "end marker line"
        );

        let lines: Vec<&str> = content.split_inclusive('\n').collect();
        assert!(lines[1].starts_with(CONFLICT_START_MARKER));
        assert_eq!(marker_body(lines[3]), CONFLICT_SEPARATOR_MARKER);
        assert!(lines[5].starts_with(CONFLICT_END_MARKER));
    }

    #[test]
    fn empty_sides_are_valid_conflicts() {
        let content = "<<<<<<< HEAD\n=======\n>>>>>>> topic\n";
        let segments = kinds(content);

        assert_eq!(segments.len(), 2);
        assert_side(&segments[0], 1, "");
        assert_side(&segments[1], 2, "");
        assert_eq!(segments[0].line_count(), 0);
        assert_eq!(segments[1].line_count(), 0);
    }

    #[test]
    fn labels_may_carry_branch_names() {
        let content = "\
<<<<<<< HEAD~3 (patch-42)
keep me
=======
drop me
>>>>>>> origin/main:note.md
";
        let segments = kinds(content);

        assert_eq!(segments.len(), 2);
        assert_side(&segments[0], 1, "keep me\n");
        assert_side(&segments[1], 3, "drop me\n");
    }

    #[test]
    fn separator_inside_incoming_side_is_content() {
        let content = "\
<<<<<<< HEAD
x
=======
y
=======
>>>>>>> topic
";
        let segments = kinds(content);

        assert_eq!(segments.len(), 2);
        assert_side(&segments[0], 1, "x\n");
        assert_side(&segments[1], 3, "y\n=======\n");
    }

    #[test]
    fn nested_opener_inside_block_is_content() {
        let content = "\
<<<<<<< HEAD
<<<<<<< fake
x
=======
y
>>>>>>> topic
";
        let segments = kinds(content);

        assert_eq!(segments.len(), 2);
        assert_side(&segments[0], 1, "<<<<<<< fake\nx\n");
        assert_side(&segments[1], 4, "y\n");
    }

    #[test]
    fn plain_content_round_trips_as_one_normal_segment() {
        let segments = kinds("");
        assert!(segments.is_empty());

        for content in ["\n", "one line", "two\nlines\n\nwith blanks\n"] {
            let segments = kinds(content);
            assert_eq!(segments.len(), 1, "content {content:?}");
            assert_normal(&segments[0], 0, content);

            let joined: String = segments
                .iter()
                .map(|segment| segment.text())
                .collect::<Vec<_>>()
                .concat();
            assert_eq!(joined, content);
        }
    }

    #[test]
    fn resolved_join_drops_only_well_formed_markers() {
        let content = format!("pre\n{BLOCK}post\n");
        let segments = kinds(&content);

        let joined: String = segments
            .iter()
            .map(|segment| segment.text())
            .collect::<Vec<_>>()
            .concat();
        assert_eq!(joined, "pre\nmine\ntheirs\npost\n");

        let malformed = "<<<<<<< HEAD\nalpha\n>>>>>>> feature\n";
        let segments = kinds(malformed);
        let joined: String = segments
            .iter()
            .map(|segment| segment.text())
            .collect::<Vec<_>>()
            .concat();
        assert_eq!(joined, malformed);
    }
}
