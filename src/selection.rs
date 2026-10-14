//! Text selection and source-faithful extraction.
//!
//! A selection is expressed in the coordinates the reader displays: a line
//! index and a display column. [`extract`] turns it back into the Markdown the
//! selected characters came from, dropping anything the renderer generated
//! (table borders, bullets, padding, code indentation) and returning table
//! cells as tab-separated rows.

use std::ops::Range;

use mdk_render::term::{Continuation, Line, LineKind};

use crate::coords;

/// A position in rendered content: a line index and a display column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Point {
    /// Index into the rendered lines.
    pub line: usize,
    /// Display column, in terminal cells.
    pub col: usize,
}

impl Point {
    /// Creates a point.
    #[must_use]
    pub const fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

/// A selection between an anchor and a head, in either direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    /// Where the selection started.
    pub anchor: Point,
    /// Where the selection currently ends.
    pub head: Point,
}

impl Selection {
    /// Starts a selection.
    #[must_use]
    pub const fn new(anchor: Point) -> Self {
        Self {
            anchor,
            head: anchor,
        }
    }

    /// The earlier end of the selection.
    #[must_use]
    pub fn start(&self) -> Point {
        self.anchor.min(self.head)
    }

    /// The later end of the selection.
    #[must_use]
    pub fn end(&self) -> Point {
        self.anchor.max(self.head)
    }

    /// Whether the selection covers nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.start() == self.end()
    }

    /// The inclusive range of line indices the selection touches.
    #[must_use]
    pub fn lines(&self) -> Range<usize> {
        let start = self.start().line;
        start..self.end().line + 1
    }

    /// Whether the cell at `line`/`column` with display `width` is selected.
    #[must_use]
    pub fn covers(&self, line: usize, column: usize, width: usize) -> bool {
        let start = self.start();
        let end = self.end();
        if line < start.line || line > end.line {
            return false;
        }
        let from = if line == start.line { start.col } else { 0 };
        let to = if line == end.line {
            end.col
        } else {
            usize::MAX
        };
        column < to && column + width > from
    }
}

/// Extracts the Markdown source of the selected region.
///
/// Lines whose characters only come from the renderer are skipped; table rows
/// are returned as tab-separated cells, and wrapped lines are joined according
/// to their [`Continuation`].
#[must_use]
pub fn extract(source: &str, lines: &[&Line], selection: &Selection) -> String {
    let start = selection.start();
    let end = selection.end();
    if lines.is_empty() || start.line >= lines.len() {
        return String::new();
    }
    let mut out = String::new();
    let mut first = true;
    let mut row_open = false;
    let last = end.line.min(lines.len() - 1);
    for (offset, line) in lines[start.line..=last].iter().enumerate() {
        let line_index = start.line + offset;
        let from = if line_index == start.line {
            start.col
        } else {
            0
        };
        let to = if line_index == end.line {
            end.col
        } else {
            usize::MAX
        };
        match line.kind {
            LineKind::TableBorder | LineKind::Rule => continue,
            LineKind::TableRow => {
                let row = extract_row(source, line, from, to);
                if row_open && line.continuation != Continuation::None {
                    merge_row(&mut out, &row);
                } else {
                    if !first {
                        out.push('\n');
                    }
                    out.push_str(&row);
                    row_open = true;
                }
            }
            _ => {
                if !first {
                    match line.continuation {
                        Continuation::None => out.push('\n'),
                        Continuation::Space => {
                            if needs_space(&out) {
                                out.push(' ');
                            }
                        }
                        Continuation::Tight => {}
                    }
                }
                out.push_str(&line_fragment(source, line, from, to));
                row_open = false;
            }
        }
        first = false;
    }
    out
}

/// Extracts the raw Markdown source covered by the selection.
///
/// This returns the smallest source range containing every selected character
/// that maps back to the file, so markup inside the selection is kept while
/// renderer decorations are still excluded.
#[must_use]
pub fn raw_extract(source: &str, lines: &[&Line], selection: &Selection) -> String {
    let mut found: Option<Range<usize>> = None;
    for line_index in selection.lines() {
        let Some(line) = lines.get(line_index) else {
            continue;
        };
        for cell in coords::cells(line) {
            if cell.width == 0 || !selection.covers(line_index, cell.column, cell.width) {
                continue;
            }
            if let Some(range) = cell.source {
                found = Some(match found {
                    Some(current) => current.start.min(range.start)..current.end.max(range.end),
                    None => range,
                });
            }
        }
    }
    found.map_or_else(String::new, |range| source[range].to_string())
}

/// Extracts the selected characters of a prose or code line.
fn line_fragment(source: &str, line: &Line, from: usize, to: usize) -> String {
    let mut out = String::new();
    let mut gap_space = false;
    for cell in coords::cells(line) {
        if cell.width == 0 {
            continue;
        }
        if cell.column >= to || cell.column + cell.width <= from {
            continue;
        }
        match cell.source {
            Some(range) => {
                if gap_space && needs_space(&out) {
                    out.push(' ');
                }
                gap_space = false;
                out.push_str(&source[range]);
            }
            None => {
                if cell.ch.is_whitespace() {
                    gap_space = true;
                }
            }
        }
    }
    out
}

/// Extracts a table row as tab-separated cells.
///
/// A cell is included whole when the selection touches any of its columns, so
/// copying part of a table never produces half a value.
fn extract_row(source: &str, line: &Line, from: usize, to: usize) -> String {
    let mut cells = Vec::new();
    for cell in &line.cells {
        if cell.columns.end <= from || cell.columns.start >= to {
            continue;
        }
        cells.push(extract_cell(source, line, &cell.columns));
    }
    cells.join("\t")
}

/// Extracts the text of one table cell.
fn extract_cell(source: &str, line: &Line, columns: &Range<usize>) -> String {
    line_fragment(source, line, columns.start, columns.end)
}

/// Appends the continuation of a wrapped table row to the row already
/// written.
fn merge_row(out: &mut String, row: &str) {
    let start = out.rfind('\n').map_or(0, |index| index + 1);
    let existing: Vec<&str> = out[start..].split('\t').collect();
    let extra: Vec<&str> = row.split('\t').collect();
    let width = existing.len().max(extra.len());
    let mut merged = Vec::with_capacity(width);
    for index in 0..width {
        match (existing.get(index), extra.get(index)) {
            (Some(left), Some(right)) => {
                if left.is_empty() {
                    merged.push((*right).to_string());
                } else if right.is_empty() {
                    merged.push((*left).to_string());
                } else {
                    merged.push(format!("{left} {right}"));
                }
            }
            (Some(left), None) => merged.push((*left).to_string()),
            (None, Some(right)) => merged.push((*right).to_string()),
            (None, None) => {}
        }
    }
    out.truncate(start);
    out.push_str(&merged.join("\t"));
}

fn needs_space(out: &str) -> bool {
    !out.is_empty() && !out.ends_with(char::is_whitespace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdk_core::Dialect;
    use mdk_render::term::Document;
    use mdk_render::{TermOptions, render_terminal_document};

    fn render(markdown: &str, width: usize) -> Document {
        let options = TermOptions {
            width,
            ..TermOptions::default()
        };
        render_terminal_document(markdown, Dialect::Gfm, &options)
    }

    fn extract_all(markdown: &str, width: usize) -> String {
        let document = render(markdown, width);
        let lines: Vec<&Line> = document.lines.iter().collect();
        let end = lines.len().saturating_sub(1);
        let selection = Selection {
            anchor: Point::new(0, 0),
            head: Point::new(end, usize::MAX),
        };
        extract(markdown, &lines, &selection)
    }

    #[test]
    fn drops_inline_markup() {
        assert_eq!(
            extract_all("Hello **bold** and `code`.\n", 80),
            "Hello bold and code."
        );
    }

    #[test]
    fn drops_heading_markers_and_list_bullets() {
        assert_eq!(extract_all("# Title\n", 80), "Title");
        assert_eq!(extract_all("- one\n- two\n", 80), "one\ntwo");
    }

    #[test]
    fn keeps_code_without_fences_or_borders() {
        assert_eq!(
            extract_all("```rust\nlet x = 1;\n\nlet y = 2;\n```\n", 80),
            "let x = 1;\n\nlet y = 2;"
        );
    }

    #[test]
    fn returns_tables_as_tab_separated_rows() {
        let text = extract_all("| a | b |\n|---|---|\n| 1 | 2 |\n", 80);
        assert_eq!(text, "a\tb\n1\t2");
    }

    #[test]
    fn table_cells_keep_their_spaces() {
        let text = extract_all("| a | b |\n|---|---|\n| one two | three |\n", 80);
        assert_eq!(text, "a\tb\none two\tthree");
    }

    #[test]
    fn merges_wrapped_table_rows() {
        let text = extract_all("| a | b |\n|---|---|\n| one two three four | x |\n", 20);
        assert_eq!(text, "a\tb\none two three four\tx");
    }

    #[test]
    fn joins_wrapped_prose_with_a_space() {
        let text = extract_all("one two three four five six\n", 20);
        assert_eq!(text, "one two three four five six");
    }

    #[test]
    fn partial_selection_keeps_words_intact() {
        let document = render("Hello world, again\n", 80);
        let lines: Vec<&Line> = document.lines.iter().collect();
        let selection = Selection {
            anchor: Point::new(0, 6),
            head: Point::new(0, 11),
        };
        assert_eq!(extract("Hello world, again\n", &lines, &selection), "world");
    }

    #[test]
    fn selection_across_blocks_keeps_blank_lines() {
        let markdown = "first paragraph\n\nsecond paragraph\n";
        let text = extract_all(markdown, 80);
        assert_eq!(text, "first paragraph\n\nsecond paragraph");
    }

    #[test]
    fn selection_of_a_single_cell_returns_its_text() {
        let markdown = "| a | b |\n|---|---|\n| 1 | 2 |\n";
        let document = render(markdown, 80);
        let lines: Vec<&Line> = document.lines.iter().collect();
        let row = lines
            .iter()
            .position(|line| line.kind == LineKind::TableRow)
            .expect("row");
        let selection = Selection {
            anchor: Point::new(row, 1),
            head: Point::new(row, 6),
        };
        assert_eq!(extract(markdown, &lines, &selection), "a");
    }

    #[test]
    fn quotes_lose_their_gutter() {
        assert_eq!(extract_all("> quoted text\n", 80), "quoted text");
    }

    #[test]
    fn empty_selection_extracts_nothing() {
        let markdown = "# Title\n";
        let document = render(markdown, 80);
        let lines: Vec<&Line> = document.lines.iter().collect();
        let point = Point::new(0, 0);
        let selection = Selection::new(point);
        assert!(selection.is_empty());
        assert_eq!(extract(markdown, &lines, &selection), "");
    }

    #[test]
    fn covers_only_the_selected_columns() {
        let selection = Selection {
            anchor: Point::new(1, 2),
            head: Point::new(3, 5),
        };
        assert!(!selection.covers(0, 0, 1));
        assert!(!selection.covers(1, 0, 1));
        assert!(selection.covers(1, 2, 1));
        assert!(selection.covers(2, 0, 1));
        assert!(selection.covers(3, 4, 1));
        assert!(!selection.covers(3, 5, 1));
        assert!(!selection.covers(4, 0, 1));
    }
}
