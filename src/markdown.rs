/* markdown.rs
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

//! Pure per-line markdown classification for the editor styling pass.
//!
//! The editor renders markdown in place: every buffer line is classified
//! into a block kind and window.rs maps the kinds onto text tags. Inline
//! emphasis, links, and checkbox state are classified the same way, into
//! per-line tag ranges. Keeping the classification free of GTK lets the
//! rules — most importantly setext heading underlines versus thematic
//! breaks — be unit tested directly.

/// Block classification of a single markdown line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineClass {
    /// Empty (whitespace-only) line.
    Blank,
    /// Fenced-code delimiter line (` ``` `).
    CodeFence,
    /// Line inside a fenced code block.
    CodeLine,
    /// ATX heading (`# Title`); `level` is 1..=6, `marker_len` the number
    /// of characters the `#`s and the following space occupy.
    Heading {
        level: u8,
        marker_len: usize,
    },
    /// Setext heading underline: a line of `-` characters directly under a
    /// non-blank paragraph line. The heading text is the previous line.
    SetextUnderline { level: u8 },
    /// Thematic break: a line consisting only of `-`, `*`, or `_`
    /// characters (at least three, ignoring interior spaces), in a position
    /// where it is not a setext underline.
    ThematicBreak,
    /// Blockquote line (starts with `> `).
    Blockquote,
    /// Task-list line (`- [ ] todo`, `- [x] done`).
    Checkbox {
        marker_len: usize,
        checked: bool,
    },
    /// Bullet-list line.
    Unordered { marker_len: usize },
    /// Numbered-list line.
    Ordered { marker_len: usize },
    /// Plain paragraph text, possibly a continuation line.
    Paragraph,
}

/// Classifies every line of `text`, in order, carrying the context the
/// setext rule needs: a line of dashes is a setext H2 underline when the
/// previous line is a non-blank paragraph line, and a thematic break
/// otherwise (including when preceded by a blank line). `***` and `___`
/// lines are always thematic breaks.
pub fn classify_lines(text: &str) -> Vec<LineClass> {
    let mut classes = Vec::new();
    let mut in_code = false;
    let mut previous: Option<LineClass> = None;

    for line in text.lines() {
        let class = classify_line(line, previous, &mut in_code);
        previous = Some(class);
        classes.push(class);
    }

    classes
}

fn classify_line(line: &str, previous: Option<LineClass>, in_code: &mut bool) -> LineClass {
    let trimmed = line.trim_end();

    if trimmed.starts_with("```") {
        *in_code = !*in_code;
        return LineClass::CodeFence;
    }
    if *in_code {
        return LineClass::CodeLine;
    }
    if trimmed.is_empty() {
        return LineClass::Blank;
    }
    if let Some((level, marker_len)) = parse_heading(trimmed) {
        return LineClass::Heading {
            level,
            marker_len,
        };
    }
    if trimmed.starts_with("> ") {
        return LineClass::Blockquote;
    }
    // Per CommonMark a `---` line is a setext H2 underline when it directly
    // underlines a paragraph line, and a thematic break anywhere else.
    if is_setext_underline(line) && matches!(previous, Some(LineClass::Paragraph)) {
        return LineClass::SetextUnderline { level: 2 };
    }
    if is_horizontal_rule(trimmed) {
        return LineClass::ThematicBreak;
    }
    if let Some((marker_len, checked)) = parse_checkbox_item(trimmed) {
        return LineClass::Checkbox {
            marker_len,
            checked,
        };
    }
    if let Some(marker_len) = parse_unordered_list_item(trimmed) {
        return LineClass::Unordered { marker_len };
    }
    if let Some(marker_len) = parse_ordered_list_item(trimmed) {
        return LineClass::Ordered { marker_len };
    }
    LineClass::Paragraph
}

fn parse_heading(line: &str) -> Option<(u8, usize)> {
    let hashes = line.chars().take_while(|ch| *ch == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }

    line.get(hashes..)
        .and_then(|rest| rest.strip_prefix(' '))
        .map(|_| (hashes as u8, hashes + 1))
}

/// A setext underline is a run of `-` characters only (one or more), with
/// optional indentation and trailing spaces.
fn is_setext_underline(line: &str) -> bool {
    let compact = line.trim();
    !compact.is_empty() && compact.chars().all(|ch| ch == '-')
}

fn is_horizontal_rule(line: &str) -> bool {
    let compact: String = line.chars().filter(|ch| !ch.is_whitespace()).collect();
    if compact.len() < 3 {
        return false;
    }

    let mut chars = compact.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    matches!(first, '-' | '*' | '_') && chars.all(|ch| ch == first)
}

fn parse_unordered_list_item(line: &str) -> Option<usize> {
    ["• ", "- ", "* ", "+ "]
        .into_iter()
        .find_map(|prefix| line.strip_prefix(prefix).map(|_| prefix.chars().count()))
}

fn parse_ordered_list_item(line: &str) -> Option<usize> {
    let dot_index = line.find(". ")?;
    let (number, rest) = line.split_at(dot_index);
    if number.chars().all(|ch| ch.is_ascii_digit()) {
        rest.strip_prefix(". ")
            .map(|_| line[..(dot_index + 2)].chars().count())
    } else {
        None
    }
}

pub(crate) fn parse_checkbox_item(line: &str) -> Option<(usize, bool)> {
    [
        "- [ ] ", "* [ ] ", "+ [ ] ", "- [x] ", "* [x] ", "+ [x] ", "- [X] ", "* [X] ",
        "+ [X] ",
    ]
    .into_iter()
    .find_map(|prefix| {
        line.strip_prefix(prefix)
            .map(|_| (prefix.chars().count(), matches!(prefix.as_bytes().get(3), Some(b'x' | b'X'))))
    })
}

/// A markdown link `[label](url)` starting at the beginning of `text`;
/// `label_len` is the label's character count (brackets excluded) and
/// `total_len` the whole match's character count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkMatch {
    pub label_len: usize,
    pub total_len: usize,
}

/// Parses a markdown link that must start at byte 0 of `text`; empty
/// labels or URLs, and unbalanced brackets, yield `None`.
pub fn parse_link_at(text: &str) -> Option<LinkMatch> {
    if !text.starts_with('[') {
        return None;
    }

    let close_label = text.find(']')?;
    let after_label = text.get((close_label + 1)..)?;
    if !after_label.starts_with('(') {
        return None;
    }

    let close_url_rel = after_label.find(')')?;
    if close_label <= 1 || close_url_rel <= 1 {
        return None;
    }

    let total_byte_len = close_label + 1 + close_url_rel + 1;

    Some(LinkMatch {
        label_len: text[1..close_label].chars().count(),
        total_len: text[..total_byte_len].chars().count(),
    })
}

/// Inline styles the editor styling pass recognizes inside a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineTag {
    /// Marker characters (`**`, `*`, `~~`, `` ` ``, `[`, `](url)`).
    Syntax,
    /// `**bold**` content.
    Bold,
    /// `*italic*` content.
    Italic,
    /// `~~strikethrough~~` content.
    Strikethrough,
    /// `` `code` `` content.
    Code,
    /// Link label content.
    Link,
    /// The remainder of a checked task-list line.
    Checked,
}

/// A tag applied to a character range of a single line; `start` and
/// `end` are character offsets, `start < end` not guaranteed (a pair of
/// touching markers yields an empty content range).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InlineRange {
    pub tag: InlineTag,
    pub start: usize,
    pub end: usize,
}

/// Computes the inline tag ranges for one line, in the order the styling
/// pass applies them: each closed emphasis pair emits opening marker,
/// content, closing marker; a link emits `[`, label, `](url)`; a checked
/// checkbox line appends one range over the text after its marker. An
/// unclosed opener at end of line contributes nothing.
pub fn inline_ranges(line: &str) -> Vec<InlineRange> {
    let mut ranges = Vec::new();
    let mut bold_start = None;
    let mut italic_start = None;
    let mut strike_start = None;
    let mut code_start = None;
    let positions: Vec<(usize, usize, char)> = line
        .char_indices()
        .enumerate()
        .map(|(char_index, (byte_index, ch))| (char_index, byte_index, ch))
        .collect();

    let mut index = 0usize;
    while index < positions.len() {
        let (char_index, byte_index, ch) = positions[index];

        if line[byte_index..].starts_with("**") {
            if let Some(start) = bold_start.take() {
                ranges.push(InlineRange {
                    tag: InlineTag::Syntax,
                    start,
                    end: start + 2,
                });
                ranges.push(InlineRange {
                    tag: InlineTag::Bold,
                    start: start + 2,
                    end: char_index,
                });
                ranges.push(InlineRange {
                    tag: InlineTag::Syntax,
                    start: char_index,
                    end: char_index + 2,
                });
            } else {
                bold_start = Some(char_index);
            }
            index += 2;
            continue;
        }

        if ch == '*' {
            if let Some(start) = italic_start.take() {
                ranges.push(InlineRange {
                    tag: InlineTag::Syntax,
                    start,
                    end: start + 1,
                });
                ranges.push(InlineRange {
                    tag: InlineTag::Italic,
                    start: start + 1,
                    end: char_index,
                });
                ranges.push(InlineRange {
                    tag: InlineTag::Syntax,
                    start: char_index,
                    end: char_index + 1,
                });
            } else {
                italic_start = Some(char_index);
            }
            index += 1;
            continue;
        }

        if line[byte_index..].starts_with("~~") {
            if let Some(start) = strike_start.take() {
                ranges.push(InlineRange {
                    tag: InlineTag::Syntax,
                    start,
                    end: start + 2,
                });
                ranges.push(InlineRange {
                    tag: InlineTag::Strikethrough,
                    start: start + 2,
                    end: char_index,
                });
                ranges.push(InlineRange {
                    tag: InlineTag::Syntax,
                    start: char_index,
                    end: char_index + 2,
                });
            } else {
                strike_start = Some(char_index);
            }
            index += 2;
            continue;
        }

        if ch == '`' {
            if let Some(start) = code_start.take() {
                ranges.push(InlineRange {
                    tag: InlineTag::Syntax,
                    start,
                    end: start + 1,
                });
                ranges.push(InlineRange {
                    tag: InlineTag::Code,
                    start: start + 1,
                    end: char_index,
                });
                ranges.push(InlineRange {
                    tag: InlineTag::Syntax,
                    start: char_index,
                    end: char_index + 1,
                });
            } else {
                code_start = Some(char_index);
            }
            index += 1;
            continue;
        }

        if ch == '[' {
            if let Some(link) = parse_link_at(&line[byte_index..]) {
                ranges.push(InlineRange {
                    tag: InlineTag::Syntax,
                    start: char_index,
                    end: char_index + 1,
                });
                ranges.push(InlineRange {
                    tag: InlineTag::Link,
                    start: char_index + 1,
                    end: char_index + 1 + link.label_len,
                });
                ranges.push(InlineRange {
                    tag: InlineTag::Syntax,
                    start: char_index + 1 + link.label_len,
                    end: char_index + link.total_len,
                });
                index += link.total_len;
                continue;
            }
        }

        index += 1;
    }

    if let Some((marker_len, true)) = parse_checkbox_item(line) {
        ranges.push(InlineRange {
            tag: InlineTag::Checked,
            start: marker_len,
            end: line.chars().count(),
        });
    }

    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classes(text: &str) -> Vec<LineClass> {
        classify_lines(text)
    }

    // --- setext headings vs thematic breaks (T26) -------------------------

    #[test]
    fn paragraph_then_dashes_is_setext_h2() {
        assert_eq!(
            classes("hello\n---"),
            vec![LineClass::Paragraph, LineClass::SetextUnderline { level: 2 }]
        );
    }

    #[test]
    fn blank_line_then_dashes_is_thematic_break() {
        assert_eq!(
            classes("hello\n\n---"),
            vec![LineClass::Paragraph, LineClass::Blank, LineClass::ThematicBreak]
        );
    }

    #[test]
    fn leading_blank_then_dashes_is_thematic_break() {
        assert_eq!(
            classes("\n---"),
            vec![LineClass::Blank, LineClass::ThematicBreak]
        );
    }

    #[test]
    fn document_start_dashes_is_thematic_break() {
        assert_eq!(classes("---"), vec![LineClass::ThematicBreak]);
    }

    #[test]
    fn stars_and_underscores_are_never_setext() {
        assert_eq!(
            classes("hello\n***"),
            vec![LineClass::Paragraph, LineClass::ThematicBreak]
        );
        assert_eq!(
            classes("hello\n___"),
            vec![LineClass::Paragraph, LineClass::ThematicBreak]
        );
        assert_eq!(
            classes("___"),
            vec![LineClass::ThematicBreak]
        );
        assert_eq!(
            classes("\n***"),
            vec![LineClass::Blank, LineClass::ThematicBreak]
        );
    }

    #[test]
    fn single_dash_under_paragraph_is_setext() {
        assert_eq!(
            classes("hello\n-"),
            vec![LineClass::Paragraph, LineClass::SetextUnderline { level: 2 }]
        );
        assert_eq!(
            classes("hello\n--"),
            vec![LineClass::Paragraph, LineClass::SetextUnderline { level: 2 }]
        );
    }

    #[test]
    fn short_dash_line_without_paragraph_is_plain() {
        // Two dashes are neither a rule (needs three) nor a setext
        // underline (no paragraph above): they stay paragraph text, as
        // before the extraction.
        assert_eq!(classes("--"), vec![LineClass::Paragraph]);
        assert_eq!(
            classes("\n--"),
            vec![LineClass::Blank, LineClass::Paragraph]
        );
    }

    #[test]
    fn setext_underline_requires_plain_paragraph_above() {
        assert_eq!(
            classes("# head\n---"),
            vec![
                LineClass::Heading {
                    level: 1,
                    marker_len: 2
                },
                LineClass::ThematicBreak
            ]
        );
        assert_eq!(
            classes("- item\n---"),
            vec![
                LineClass::Unordered { marker_len: 2 },
                LineClass::ThematicBreak
            ]
        );
        assert_eq!(
            classes("> quote\n---"),
            vec![LineClass::Blockquote, LineClass::ThematicBreak]
        );
        assert_eq!(
            classes("1. item\n---"),
            vec![LineClass::Ordered { marker_len: 3 }, LineClass::ThematicBreak]
        );
    }

    #[test]
    fn code_fence_shields_dashes_from_setext() {
        assert_eq!(
            classes("```\ncode\n---\n```"),
            vec![
                LineClass::CodeFence,
                LineClass::CodeLine,
                LineClass::CodeLine,
                LineClass::CodeFence
            ]
        );
    }

    #[test]
    fn break_after_setext_underline_is_a_break() {
        assert_eq!(
            classes("a\n---\n---"),
            vec![
                LineClass::Paragraph,
                LineClass::SetextUnderline { level: 2 },
                LineClass::ThematicBreak
            ]
        );
    }

    #[test]
    fn each_paragraph_line_can_carry_its_own_setext() {
        assert_eq!(
            classes("a\n---\nb\n---"),
            vec![
                LineClass::Paragraph,
                LineClass::SetextUnderline { level: 2 },
                LineClass::Paragraph,
                LineClass::SetextUnderline { level: 2 }
            ]
        );
    }

    #[test]
    fn spaced_dash_lines_are_breaks_not_setext() {
        // Interior spaces disqualify a line as a setext underline; the
        // rule matcher (which ignores spaces) still takes it.
        assert_eq!(
            classes("hello\n- - -"),
            vec![LineClass::Paragraph, LineClass::ThematicBreak]
        );
        assert_eq!(
            classes("* * *"),
            vec![LineClass::ThematicBreak]
        );
        assert_eq!(
            classes("_ _ _"),
            vec![LineClass::ThematicBreak]
        );
    }

    // --- behavior freeze: pre-existing classifications ---------------------

    #[test]
    fn atx_headings() {
        assert_eq!(
            classes("# T"),
            vec![LineClass::Heading {
                level: 1,
                marker_len: 2
            }]
        );
        assert_eq!(
            classes("## T"),
            vec![LineClass::Heading {
                level: 2,
                marker_len: 3
            }]
        );
        assert_eq!(
            classes("###### T"),
            vec![LineClass::Heading {
                level: 6,
                marker_len: 7
            }]
        );
        // No space after the hashes: not a heading.
        assert_eq!(classes("#T"), vec![LineClass::Paragraph]);
        // Seven hashes: not a heading.
        assert_eq!(classes("####### T"), vec![LineClass::Paragraph]);
    }

    #[test]
    fn code_fences_toggle() {
        assert_eq!(
            classes("```\ncode\n```"),
            vec![
                LineClass::CodeFence,
                LineClass::CodeLine,
                LineClass::CodeFence
            ]
        );
        // Unclosed fence: everything after stays code.
        assert_eq!(
            classes("a\n```\nb"),
            vec![
                LineClass::Paragraph,
                LineClass::CodeFence,
                LineClass::CodeLine
            ]
        );
        // Blank lines inside a fence are code lines.
        assert_eq!(
            classes("```\n\n```"),
            vec![
                LineClass::CodeFence,
                LineClass::CodeLine,
                LineClass::CodeFence
            ]
        );
    }

    #[test]
    fn blockquotes_need_space_after_gt() {
        assert_eq!(classes("> x"), vec![LineClass::Blockquote]);
        assert_eq!(classes(">x"), vec![LineClass::Paragraph]);
    }

    #[test]
    fn list_items() {
        assert_eq!(
            classes("- x"),
            vec![LineClass::Unordered { marker_len: 2 }]
        );
        assert_eq!(
            classes("* x"),
            vec![LineClass::Unordered { marker_len: 2 }]
        );
        assert_eq!(
            classes("+ x"),
            vec![LineClass::Unordered { marker_len: 2 }]
        );
        assert_eq!(
            classes("• x"),
            vec![LineClass::Unordered { marker_len: 2 }]
        );
        assert_eq!(
            classes("1. x"),
            vec![LineClass::Ordered { marker_len: 3 }]
        );
        assert_eq!(
            classes("12. x"),
            vec![LineClass::Ordered { marker_len: 4 }]
        );
        // Not a number before the dot: not an ordered item.
        assert_eq!(classes("a. x"), vec![LineClass::Paragraph]);
    }

    #[test]
    fn checkbox_items() {
        assert_eq!(
            classes("- [ ] x"),
            vec![LineClass::Checkbox {
                marker_len: 6,
                checked: false
            }]
        );
        assert_eq!(
            classes("- [x] x"),
            vec![LineClass::Checkbox {
                marker_len: 6,
                checked: true
            }]
        );
        assert_eq!(
            classes("* [X] x"),
            vec![LineClass::Checkbox {
                marker_len: 6,
                checked: true
            }]
        );
    }

    #[test]
    fn blank_and_plain_lines() {
        // An empty document has no lines at all.
        assert_eq!(classes(""), Vec::new());
        assert_eq!(classes("\n"), vec![LineClass::Blank]);
        assert_eq!(classes("   "), vec![LineClass::Blank]);
        assert_eq!(classes("hello"), vec![LineClass::Paragraph]);
        assert_eq!(
            classes("a\nb"),
            vec![LineClass::Paragraph, LineClass::Paragraph]
        );
    }

    // --- inline tag ranges (T35) ------------------------------------------

    fn ranges(text: &str) -> Vec<(InlineTag, usize, usize)> {
        inline_ranges(text)
            .into_iter()
            .map(|range| (range.tag, range.start, range.end))
            .collect()
    }

    #[test]
    fn bold_pair() {
        assert_eq!(
            ranges("**bold**"),
            vec![
                (InlineTag::Syntax, 0, 2),
                (InlineTag::Bold, 2, 6),
                (InlineTag::Syntax, 6, 8)
            ]
        );
    }

    #[test]
    fn italic_pair() {
        assert_eq!(
            ranges("*italic*"),
            vec![
                (InlineTag::Syntax, 0, 1),
                (InlineTag::Italic, 1, 7),
                (InlineTag::Syntax, 7, 8)
            ]
        );
    }

    #[test]
    fn strike_pair() {
        assert_eq!(
            ranges("~~strike~~"),
            vec![
                (InlineTag::Syntax, 0, 2),
                (InlineTag::Strikethrough, 2, 8),
                (InlineTag::Syntax, 8, 10)
            ]
        );
    }

    #[test]
    fn code_pair() {
        assert_eq!(
            ranges("`code`"),
            vec![
                (InlineTag::Syntax, 0, 1),
                (InlineTag::Code, 1, 5),
                (InlineTag::Syntax, 5, 6)
            ]
        );
    }

    #[test]
    fn unclosed_openers_emit_nothing() {
        assert_eq!(ranges("**bold"), Vec::new());
        assert_eq!(ranges("*ital"), Vec::new());
        assert_eq!(ranges("~~str"), Vec::new());
        assert_eq!(ranges("`cod"), Vec::new());
    }

    #[test]
    fn touching_markers_yield_empty_content_range() {
        // `****` opens and closes on itself; the empty bold content range
        // is emitted but filtered out by the tag application loop.
        assert_eq!(
            ranges("****"),
            vec![
                (InlineTag::Syntax, 0, 2),
                (InlineTag::Bold, 2, 2),
                (InlineTag::Syntax, 2, 4)
            ]
        );
    }

    #[test]
    fn bold_nested_italic_emits_inner_pair_first() {
        assert_eq!(
            ranges("**a *b* c**"),
            vec![
                (InlineTag::Syntax, 4, 5),
                (InlineTag::Italic, 5, 6),
                (InlineTag::Syntax, 6, 7),
                (InlineTag::Syntax, 0, 2),
                (InlineTag::Bold, 2, 9),
                (InlineTag::Syntax, 9, 11)
            ]
        );
    }

    #[test]
    fn italic_around_bold_spans_the_whole_line() {
        assert_eq!(
            ranges("*a **b** c*"),
            vec![
                (InlineTag::Syntax, 3, 5),
                (InlineTag::Bold, 5, 6),
                (InlineTag::Syntax, 6, 8),
                (InlineTag::Syntax, 0, 1),
                (InlineTag::Italic, 1, 10),
                (InlineTag::Syntax, 10, 11)
            ]
        );
    }

    #[test]
    fn link_ranges() {
        assert_eq!(
            ranges("see [label](url) now"),
            vec![
                (InlineTag::Syntax, 4, 5),
                (InlineTag::Link, 5, 10),
                (InlineTag::Syntax, 10, 16)
            ]
        );
    }

    #[test]
    fn links_need_nonempty_label_and_url() {
        assert_eq!(ranges("[](u)"), Vec::new());
        assert_eq!(ranges("[a]()"), Vec::new());
        assert_eq!(ranges("[a]b"), Vec::new());
        assert_eq!(ranges("[a"), Vec::new());
    }

    #[test]
    fn link_offsets_are_chars_not_bytes() {
        // `é` is two bytes wide; ranges stay in character space.
        assert_eq!(
            ranges("[é](u)"),
            vec![
                (InlineTag::Syntax, 0, 1),
                (InlineTag::Link, 1, 2),
                (InlineTag::Syntax, 2, 6)
            ]
        );
    }

    #[test]
    fn parse_link_at_requires_full_shape() {
        assert_eq!(parse_link_at("x](y)"), None);
        assert_eq!(
            parse_link_at("[a](b)"),
            Some(LinkMatch {
                label_len: 1,
                total_len: 6
            })
        );
    }

    #[test]
    fn checked_checkbox_covers_remainder_of_line() {
        assert_eq!(ranges("- [x] done"), vec![(InlineTag::Checked, 6, 10)]);
        assert_eq!(ranges("- [X] done"), vec![(InlineTag::Checked, 6, 10)]);
    }

    #[test]
    fn unchecked_or_unlisted_brackets_have_no_ranges() {
        assert_eq!(ranges("- [ ] todo"), Vec::new());
        assert_eq!(ranges("[x] not a list"), Vec::new());
    }

    #[test]
    fn realistic_mixed_line_keeps_application_order() {
        assert_eq!(
            ranges("**T** and *i* and ~~s~~ and `c` and [l](u)"),
            vec![
                (InlineTag::Syntax, 0, 2),
                (InlineTag::Bold, 2, 3),
                (InlineTag::Syntax, 3, 5),
                (InlineTag::Syntax, 10, 11),
                (InlineTag::Italic, 11, 12),
                (InlineTag::Syntax, 12, 13),
                (InlineTag::Syntax, 18, 20),
                (InlineTag::Strikethrough, 20, 21),
                (InlineTag::Syntax, 21, 23),
                (InlineTag::Syntax, 28, 29),
                (InlineTag::Code, 29, 30),
                (InlineTag::Syntax, 30, 31),
                (InlineTag::Syntax, 36, 37),
                (InlineTag::Link, 37, 38),
                (InlineTag::Syntax, 38, 42)
            ]
        );
    }
}
