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

/// What a styled line range produced by [`conflict_style_spans`] represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConflictSpanKind {
    /// Lines belonging to the current side of a conflict block.
    CurrentLines,
    /// Lines belonging to the incoming side of a conflict block.
    IncomingLines,
    /// One of the three marker lines (`<<<<<<<`, `=======`, `>>>>>>>`).
    MarkerLine,
}

/// A half-open range of 0-based lines to style, derived from a parsed conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ConflictSpan {
    pub(crate) kind: ConflictSpanKind,
    pub(crate) start_line: usize,
    /// Exclusive.
    pub(crate) end_line: usize,
}

/// Maps parsed conflict segments into styleable line ranges.
///
/// Every well-formed block yields one span per non-empty side plus three
/// single-line marker spans; blocks the parser demoted to prose yield
/// nothing, so hand-written marker lookalikes are never styled.
pub(crate) fn conflict_style_spans(content: &str) -> Vec<ConflictSpan> {
    let mut spans = Vec::new();

    for segment in split_conflict_segments(content) {
        let start = segment.start_line();
        let end = start + segment.line_count();
        match segment {
            ConflictSegment::Normal { .. } => {}
            ConflictSegment::Current { .. } => {
                spans.push(marker_span(start - 1));
                if end > start {
                    spans.push(ConflictSpan {
                        kind: ConflictSpanKind::CurrentLines,
                        start_line: start,
                        end_line: end,
                    });
                }
                spans.push(marker_span(end));
            }
            ConflictSegment::Incoming { .. } => {
                if end > start {
                    spans.push(ConflictSpan {
                        kind: ConflictSpanKind::IncomingLines,
                        start_line: start,
                        end_line: end,
                    });
                }
                spans.push(marker_span(end));
            }
        }
    }

    spans
}

fn marker_span(line: usize) -> ConflictSpan {
    ConflictSpan {
        kind: ConflictSpanKind::MarkerLine,
        start_line: line,
        end_line: line + 1,
    }
}

/// Which side of a conflict block survives when the block is accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConflictSide {
    /// The lines between `<<<<<<<` and `=======`.
    Current,
    /// The lines between `=======` and `>>>>>>>`.
    Incoming,
}

/// The splice [`ConflictBlock::resolve`] proposes: delete every line of the
/// block (all three marker lines plus both sides) and put the surviving
/// side's verbatim text back in its place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConflictResolution {
    /// First line of the block — the `<<<<<<<` header, inclusive.
    pub(crate) start_line: usize,
    /// End of the block — the line after `>>>>>>>`, exclusive.
    pub(crate) end_line: usize,
    /// Verbatim surviving-side text, including its trailing newline;
    /// empty when that side had no lines.
    pub(crate) replacement: String,
}

/// A well-formed conflict block located inside entry content.
///
/// Line numbers are 0-based and cover the whole block: the `<<<<<<<`
/// header, both sides, `=======`, and `>>>>>>>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConflictBlock {
    pub(crate) start_line: usize,
    /// Exclusive.
    pub(crate) end_line: usize,
    pub(crate) current_text: String,
    pub(crate) incoming_text: String,
}

impl ConflictBlock {
    /// Plans the deletion of this whole block, keeping `side`'s lines.
    ///
    /// The losing side's lines and all three marker lines disappear; the
    /// winning side's text is preserved byte-for-byte so hand edits made
    /// inside the block survive acceptance.
    pub(crate) fn resolve(&self, side: ConflictSide) -> ConflictResolution {
        let replacement = match side {
            ConflictSide::Current => self.current_text.clone(),
            ConflictSide::Incoming => self.incoming_text.clone(),
        };

        ConflictResolution {
            start_line: self.start_line,
            end_line: self.end_line,
            replacement,
        }
    }
}

/// Finds the well-formed conflict block containing 0-based `line`.
///
/// Every line of a block qualifies — markers included — so acting on the
/// cursor wherever it sits inside the tinted region resolves that block.
/// Plain prose and malformed marker lookalikes yield `None`.
pub(crate) fn conflict_block_at_line(content: &str, line: usize) -> Option<ConflictBlock> {
    let segments = split_conflict_segments(content);

    for pair in segments.windows(2) {
        if !matches!(
            (&pair[0], &pair[1]),
            (ConflictSegment::Current { .. }, ConflictSegment::Incoming { .. })
        ) {
            continue;
        }

        let current = &pair[0];
        let incoming = &pair[1];

        // The parser drops marker lines from segment texts, so the header
        // sits directly above the current side and the end marker directly
        // below the incoming side.
        let start_line = current.start_line() - 1;
        let end_line = incoming.start_line() + incoming.line_count() + 1;
        if line < start_line || line >= end_line {
            continue;
        }

        return Some(ConflictBlock {
            start_line,
            end_line,
            current_text: current.text().to_string(),
            incoming_text: incoming.text().to_string(),
        });
    }

    None
}

/// Counts the well-formed conflict blocks still unresolved in entry content.
///
/// Every well-formed block contributes exactly one `Current` segment, so
/// counting those segments counts blocks even when sides are empty or two
/// blocks sit back to back. The editor restyles conflict tags from this same
/// parser on every buffer change, so this matches what the applied tags show.
pub(crate) fn unresolved_conflict_count(content: &str) -> usize {
    split_conflict_segments(content)
        .iter()
        .filter(|segment| matches!(segment, ConflictSegment::Current { .. }))
        .count()
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

#[cfg(test)]
mod conflict_span_tests {
    use super::*;

    const BLOCK: &str = "\
<<<<<<< HEAD
mine
=======
theirs
>>>>>>> topic
";

    fn assert_side(span: &ConflictSpan, kind: ConflictSpanKind, start: usize, end: usize) {
        assert_eq!(span.kind, kind);
        assert_eq!(span.start_line, start);
        assert_eq!(span.end_line, end);
    }

    fn assert_marker(span: &ConflictSpan, line: usize) {
        assert_marker_range(span, line, line + 1);
    }

    fn assert_marker_range(span: &ConflictSpan, start: usize, end: usize) {
        assert_eq!(span.kind, ConflictSpanKind::MarkerLine);
        assert_eq!(span.start_line, start);
        assert_eq!(span.end_line, end);
    }

    #[test]
    fn single_block_yields_markers_and_both_sides() {
        let content = format!("intro\n{BLOCK}outro\n");
        let spans = conflict_style_spans(&content);

        assert_eq!(spans.len(), 5);
        assert_marker(&spans[0], 1);
        assert_side(&spans[1], ConflictSpanKind::CurrentLines, 2, 3);
        assert_marker(&spans[2], 3);
        assert_side(&spans[3], ConflictSpanKind::IncomingLines, 4, 5);
        assert_marker(&spans[4], 5);
    }

    #[test]
    fn multiple_blocks_stay_in_document_order() {
        let content = format!("{BLOCK}{BLOCK}tail\n");
        let spans = conflict_style_spans(&content);

        assert_eq!(spans.len(), 10);
        assert_marker(&spans[0], 0);
        assert_side(&spans[1], ConflictSpanKind::CurrentLines, 1, 2);
        assert_side(&spans[6], ConflictSpanKind::CurrentLines, 6, 7);
        assert_marker(&spans[9], 9);
        assert!(spans
            .windows(2)
            .all(|pair| pair[0].start_line <= pair[1].start_line));
    }

    #[test]
    fn block_at_file_start_still_dims_header_marker() {
        let content = format!("{BLOCK}after\n");
        let spans = conflict_style_spans(&content);

        assert_eq!(spans.len(), 5);
        assert_marker(&spans[0], 0);
        assert_side(&spans[1], ConflictSpanKind::CurrentLines, 1, 2);
        assert_marker(&spans[2], 2);
        assert_side(&spans[3], ConflictSpanKind::IncomingLines, 3, 4);
        assert_marker(&spans[4], 4);
    }

    #[test]
    fn empty_sides_produce_marker_spans_only() {
        let content = "<<<<<<< HEAD\n=======\n>>>>>>> topic\n";
        let spans = conflict_style_spans(content);

        assert_eq!(spans.len(), 3);
        assert!(spans
            .iter()
            .all(|span| span.kind == ConflictSpanKind::MarkerLine));
        assert_marker(&spans[0], 0);
        assert_marker(&spans[1], 1);
        assert_marker(&spans[2], 2);
    }

    #[test]
    fn multiline_sides_cover_their_whole_range() {
        let content = "\
<<<<<<< HEAD
a
b
=======
c
d
e
>>>>>>> topic
";
        let spans = conflict_style_spans(content);

        assert_eq!(spans.len(), 5);
        assert_side(&spans[1], ConflictSpanKind::CurrentLines, 1, 3);
        assert_side(&spans[3], ConflictSpanKind::IncomingLines, 4, 7);
    }

    #[test]
    fn malformed_and_plain_content_yield_nothing() {
        for content in [
            "",
            "just prose\n",
            "<<<<<<< HEAD\nalpha\n>>>>>>> feature\n",
            "intro\n<<<<<<< HEAD\nalpha\n=======\nbeta",
            "a\n=======\n>>>>>>> nowhere\nb\n",
            "see <<<<<<< demo below\n",
        ] {
            assert!(
                conflict_style_spans(content).is_empty(),
                "content {content:?}"
            );
        }
    }

    #[test]
    fn spans_never_escape_the_document() {
        for content in [
            BLOCK.to_string(),
            format!("pre\n{BLOCK}post\n"),
            String::from("<<<<<<< HEAD\n=======\n>>>>>>> t"),
            String::from("x\n<<<<<<< H\n=======\n>>>>>>> t"),
        ] {
            let line_count = content.lines().count().max(1);
            for span in conflict_style_spans(&content) {
                assert!(span.start_line < span.end_line);
                assert!(span.end_line <= line_count, "content {content:?}");
            }
        }
    }
}

#[cfg(test)]
mod conflict_resolve_tests {
    use super::*;

    const BLOCK: &str = "\
<<<<<<< HEAD
mine
=======
theirs
>>>>>>> topic
";

    /// Mirrors the buffer splice the editor performs: delete lines
    /// `[start_line, end_line)` and insert the replacement at `start_line`.
    fn apply_resolution(content: &str, resolution: &ConflictResolution) -> String {
        let mut out = String::new();
        for (number, line) in content.split_inclusive('\n').enumerate() {
            if number == resolution.start_line {
                out.push_str(&resolution.replacement);
            }
            if !(resolution.start_line..resolution.end_line).contains(&number) {
                out.push_str(line);
            }
        }
        out
    }

    fn block_at(content: &str, line: usize) -> ConflictBlock {
        conflict_block_at_line(content, line)
            .unwrap_or_else(|| panic!("line {line} should sit inside a block of {content:?}"))
    }

    #[test]
    fn cursor_on_any_block_line_finds_the_block() {
        let content = format!("pre\n{BLOCK}post\n");

        for line in [1usize, 2, 3, 4, 5] {
            let block = block_at(&content, line);
            assert_eq!(block.start_line, 1, "line {line}");
            assert_eq!(block.end_line, 6, "line {line}");
            assert_eq!(block.current_text, "mine\n", "line {line}");
            assert_eq!(block.incoming_text, "theirs\n", "line {line}");
        }
    }

    #[test]
    fn accept_current_keeps_only_current_side() {
        let content = format!("pre\n{BLOCK}post\n");
        let resolved = block_at(&content, 2).resolve(ConflictSide::Current);

        assert_eq!(resolved.replacement, "mine\n");
        assert_eq!(apply_resolution(&content, &resolved), "pre\nmine\npost\n");
    }

    #[test]
    fn accept_incoming_keeps_only_incoming_side() {
        let content = format!("pre\n{BLOCK}post\n");
        let resolved = block_at(&content, 4).resolve(ConflictSide::Incoming);

        assert_eq!(resolved.replacement, "theirs\n");
        assert_eq!(apply_resolution(&content, &resolved), "pre\ntheirs\npost\n");
    }

    #[test]
    fn lines_outside_blocks_do_not_resolve() {
        let content = format!("pre\n{BLOCK}post\n");

        for line in [0usize, 6] {
            assert!(
                conflict_block_at_line(&content, line).is_none(),
                "line {line}"
            );
        }
        assert!(conflict_block_at_line(&content, 7).is_none());
    }

    #[test]
    fn only_the_targeted_block_changes() {
        let content = format!("{BLOCK}{BLOCK}");
        let resolved = block_at(&content, 7).resolve(ConflictSide::Incoming);

        assert_eq!(resolved.start_line, 5);
        assert_eq!(resolved.end_line, 10);
        assert_eq!(
            apply_resolution(&content, &resolved),
            format!("{BLOCK}theirs\n")
        );
    }

    #[test]
    fn adjacent_blocks_resolve_independently() {
        let content = format!("{BLOCK}{BLOCK}");

        let first = block_at(&content, 0).resolve(ConflictSide::Current);
        assert_eq!(apply_resolution(&content, &first), format!("mine\n{BLOCK}"));

        let second = block_at(&content, 9).resolve(ConflictSide::Current);
        assert_eq!(apply_resolution(&content, &second), format!("{BLOCK}mine\n"));
    }

    #[test]
    fn multiline_sides_survive_verbatim() {
        let content = "\
<<<<<<< HEAD
a
b

c
=======
d
e
>>>>>>> topic
";
        let current = block_at(content, 3).resolve(ConflictSide::Current);
        assert_eq!(current.replacement, "a\nb\n\nc\n");
        assert_eq!(apply_resolution(content, &current), "a\nb\n\nc\n");

        let incoming = block_at(content, 3).resolve(ConflictSide::Incoming);
        assert_eq!(incoming.replacement, "d\ne\n");
    }

    #[test]
    fn empty_side_acceptance_clears_the_block() {
        let content = "<<<<<<< HEAD\n=======\n>>>>>>> topic\n";

        for side in [ConflictSide::Current, ConflictSide::Incoming] {
            let resolved = block_at(content, 1).resolve(side);
            assert_eq!(resolved.replacement, "", "{side:?}");
            assert_eq!(resolved.start_line, 0, "{side:?}");
            assert_eq!(resolved.end_line, 3, "{side:?}");
            assert_eq!(apply_resolution(content, &resolved), "", "{side:?}");
        }
    }

    #[test]
    fn empty_side_keeps_the_other_side() {
        let content = "<<<<<<< HEAD\nkept\n=======\n>>>>>>> topic\n";
        let resolved = block_at(content, 2).resolve(ConflictSide::Current);

        assert_eq!(resolved.replacement, "kept\n");
        assert_eq!(apply_resolution(content, &resolved), "kept\n");
    }

    #[test]
    fn block_at_eof_without_trailing_newline_resolves() {
        let content = "pre\n<<<<<<< H\na\n=======\nb\n>>>>>>> t";
        let resolved = block_at(content, 5).resolve(ConflictSide::Current);

        assert_eq!(resolved.start_line, 1);
        assert_eq!(resolved.end_line, 6);
        assert_eq!(apply_resolution(content, &resolved), "pre\na\n");
    }

    #[test]
    fn malformed_and_plain_content_never_resolves() {
        for content in [
            "",
            "just prose\n",
            "<<<<<<< HEAD\nalpha\n>>>>>>> feature\n",
            "intro\n<<<<<<< HEAD\nalpha\n=======\nbeta",
            "a\n=======\n>>>>>>> nowhere\nb\n",
            "see <<<<<<< demo below\n",
            "  =======\n\t>>>>>>> note\n",
        ] {
            let lines = content.lines().count().max(1);
            for line in 0..lines {
                assert!(
                    conflict_block_at_line(content, line).is_none(),
                    "content {content:?} line {line}"
                );
            }
        }
    }

    #[test]
    fn hand_edits_inside_a_side_are_what_get_kept() {
        // The user retyped "mine" as "MINE (fixed)" before accepting; that
        // edited text — not some cached original — must survive.
        let content = "<<<<<<< HEAD\nMINE (fixed)\n=======\ntheirs\n>>>>>>> topic\n";
        let resolved = block_at(content, 1).resolve(ConflictSide::Current);

        assert_eq!(resolved.replacement, "MINE (fixed)\n");
        assert_eq!(apply_resolution(content, &resolved), "MINE (fixed)\n");
    }

    #[test]
    fn resolutions_leave_no_conflict_markers_behind() {
        fn well_formed_block_count(content: &str) -> usize {
            split_conflict_segments(content)
                .iter()
                .filter(|segment| matches!(segment, ConflictSegment::Current { .. }))
                .count()
        }

        for content in [
            BLOCK.to_string(),
            format!("pre\n{BLOCK}post\n"),
            format!("{BLOCK}{BLOCK}tail\n"),
            String::from("<<<<<<< HEAD\n=======\n>>>>>>> t"),
        ] {
            let blocks_before = well_formed_block_count(&content);
            for line in 0..content.lines().count().max(1) {
                if let Some(block) = conflict_block_at_line(&content, line) {
                    for side in [ConflictSide::Current, ConflictSide::Incoming] {
                        let resolved = apply_resolution(
                            &content,
                            &block.resolve(side),
                        );
                        assert_eq!(
                            well_formed_block_count(&resolved),
                            blocks_before - 1,
                            "targeted block must disappear; {side:?} from line {line}"
                        );
                        if blocks_before == 1 {
                            assert!(
                                conflict_style_spans(&resolved).is_empty(),
                                "residual markers in {resolved:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod save_guard_tests {
    use super::*;

    const BLOCK: &str = "\
<<<<<<< HEAD
mine
=======
theirs
>>>>>>> topic
";

    #[test]
    fn clean_content_has_zero_unresolved_conflicts() {
        assert_eq!(unresolved_conflict_count(""), 0);
        assert_eq!(unresolved_conflict_count("just prose\n"), 0);
        // A lone separator line is ordinary markdown, not a conflict.
        assert_eq!(unresolved_conflict_count("title\n=======\nbody\n"), 0);
    }

    #[test]
    fn malformed_marker_lookalikes_do_not_block_saving() {
        let missing_separator = "<<<<<<< HEAD\nmine\n>>>>>>> topic\n";
        assert_eq!(unresolved_conflict_count(missing_separator), 0);

        let unclosed = "<<<<<<< HEAD\nmine\n=======\n";
        assert_eq!(unresolved_conflict_count(unclosed), 0);

        let indented = "  <<<<<<< HEAD\nmine\n=======\ntheirs\n  >>>>>>> topic\n";
        assert_eq!(unresolved_conflict_count(indented), 0);
    }

    #[test]
    fn single_block_counts_once() {
        assert_eq!(unresolved_conflict_count(BLOCK), 1);
    }

    #[test]
    fn separated_blocks_each_count() {
        let content = format!("pre\n{BLOCK}middle\n{BLOCK}post\n");
        assert_eq!(unresolved_conflict_count(&content), 2);
    }

    #[test]
    fn back_to_back_blocks_do_not_merge_into_one() {
        let content = format!("{BLOCK}{BLOCK}");
        assert_eq!(unresolved_conflict_count(&content), 2);
    }

    #[test]
    fn empty_sides_still_count_as_unresolved() {
        for content in [
            "<<<<<<< HEAD\n=======\ntheirs\n>>>>>>> topic\n",
            "<<<<<<< HEAD\nmine\n=======\n>>>>>>> topic\n",
            "<<<<<<< HEAD\n=======\n>>>>>>> topic",
        ] {
            assert_eq!(unresolved_conflict_count(content), 1, "{content:?}");
        }
    }
}
