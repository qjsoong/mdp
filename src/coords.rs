//! Mapping between display columns, rendered characters and source offsets.

use std::ops::Range;

use mdk_render::term::Line;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// A grapheme cluster on a rendered line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    /// The cluster as it is drawn.
    pub text: String,
    /// The first character of the cluster, for word classification.
    pub ch: char,
    /// Index of the span the cluster belongs to.
    pub span: usize,
    /// Byte offset of the cluster inside its span's text.
    pub byte: usize,
    /// First display column occupied by the cluster.
    pub column: usize,
    /// Display width in columns; zero for combining marks.
    pub width: usize,
    /// Source byte range the cluster displays, when it has one.
    pub source: Option<Range<usize>>,
}

impl Cell {
    /// The display columns occupied by the cluster.
    #[must_use]
    pub fn columns(&self) -> Range<usize> {
        self.column..self.column + self.width
    }
}

/// Expands a rendered line into grapheme clusters with positions.
#[must_use]
pub fn cells(line: &Line) -> Vec<Cell> {
    let mut out = Vec::new();
    let mut column = 0usize;
    for (span_index, span) in line.spans.iter().enumerate() {
        for (byte, text) in UnicodeSegmentation::grapheme_indices(span.text.as_str(), true) {
            let width = cluster_width(text);
            out.push(Cell {
                ch: text.chars().next().unwrap_or(' '),
                text: text.to_string(),
                span: span_index,
                byte,
                column,
                width,
                source: cluster_source(span, byte, text),
            });
            column += width;
        }
    }
    out
}

/// The display width of a grapheme cluster as the renderer counts it.
#[must_use]
pub fn cluster_width(text: &str) -> usize {
    if text == "\t" {
        return 1;
    }
    UnicodeWidthStr::width(text)
}

/// The display width of a character as the renderer counts it.
#[must_use]
pub fn char_width(ch: char) -> usize {
    match ch {
        '\t' => 1,
        _ => ch.width().unwrap_or(0),
    }
}

fn cluster_source(span: &mdk_render::term::Span, byte: usize, text: &str) -> Option<Range<usize>> {
    let mut range: Option<Range<usize>> = None;
    for (offset, _) in text.char_indices() {
        if let Some(source) = span.source.char_at(&span.text, byte + offset) {
            range = Some(match range {
                Some(current) => current.start.min(source.start)..current.end.max(source.end),
                None => source,
            });
        }
    }
    range
}

/// The total display width of a line.
#[must_use]
pub fn line_width(line: &Line) -> usize {
    line.spans.iter().map(mdk_render::term::Span::width).sum()
}

/// The character containing (or nearest at or after) a display column.
#[must_use]
pub fn char_at_column(line: &Line, column: usize) -> Option<Cell> {
    let cells = cells(line);
    cells
        .iter()
        .find(|cell| cell.width > 0 && cell.columns().contains(&column))
        .or_else(|| {
            cells
                .iter()
                .find(|cell| cell.width > 0 && cell.column >= column)
        })
        .or_else(|| cells.iter().rev().find(|cell| cell.width > 0))
        .cloned()
}

/// The first display column of the character after `column`.
#[must_use]
pub fn next_char_column(line: &Line, column: usize) -> Option<usize> {
    cells(line)
        .iter()
        .find(|cell| cell.width > 0 && cell.column > column)
        .map(|cell| cell.column)
}

/// The first display column of the character before `column`.
#[must_use]
pub fn prev_char_column(line: &Line, column: usize) -> Option<usize> {
    cells(line)
        .iter()
        .rev()
        .find(|cell| cell.width > 0 && cell.column < column)
        .map(|cell| cell.column)
}

/// A column clamped to the start of the last character of the line.
#[must_use]
pub fn clamp_column(line: &Line, column: usize) -> usize {
    match char_at_column(line, column) {
        Some(cell) => cell.column,
        None => 0,
    }
}

/// Whether a character can be part of a word.
#[must_use]
pub fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// The start of the word containing or preceding `column`.
#[must_use]
pub fn word_start(line: &Line, column: usize) -> usize {
    let cells = cells(line);
    let Some(mut index) = cells
        .iter()
        .position(|cell| cell.column + cell.width > column)
    else {
        return cells.last().map_or(0, |cell| cell.column);
    };
    if is_word_char(cells[index].ch) {
        while index > 0 && is_word_char(cells[index - 1].ch) {
            index -= 1;
        }
        return cells[index].column;
    }
    while index > 0 && !is_word_char(cells[index - 1].ch) {
        index -= 1;
    }
    while index > 0 && is_word_char(cells[index - 1].ch) {
        index -= 1;
    }
    cells[index].column
}

/// The start of the word after `column`.
#[must_use]
pub fn next_word_start(line: &Line, column: usize) -> usize {
    let cells = cells(line);
    let mut index = 0;
    while index < cells.len() && cells[index].column + cells[index].width <= column {
        index += 1;
    }
    while index < cells.len() && is_word_char(cells[index].ch) {
        index += 1;
    }
    while index < cells.len() && !is_word_char(cells[index].ch) {
        index += 1;
    }
    cells.get(index).map_or_else(
        || cells.last().map_or(0, |cell| cell.column),
        |cell| cell.column,
    )
}

/// The end of the word after `column`.
#[must_use]
pub fn next_word_end(line: &Line, column: usize) -> usize {
    let start = next_word_start(line, column);
    if start <= column {
        return word_end(line, column);
    }
    word_end(line, start)
}

/// The end of the word containing `column`.
#[must_use]
pub fn word_end(line: &Line, column: usize) -> usize {
    let cells = cells(line);
    let mut index = 0;
    while index < cells.len() && cells[index].column + cells[index].width <= column {
        index += 1;
    }
    while index < cells.len() && !is_word_char(cells[index].ch) {
        index += 1;
    }
    while index + 1 < cells.len() && is_word_char(cells[index + 1].ch) {
        index += 1;
    }
    cells.get(index).map_or_else(
        || cells.last().map_or(0, |cell| cell.column),
        |cell| cell.column,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdk_render::term::{Continuation, LineKind, SourceMap, Span, Style};

    fn line(text: &str) -> Line {
        Line {
            spans: vec![Span::mapped(Style::default(), text, SourceMap::Verbatim(0))],
            kind: LineKind::Text,
            block: None,
            continuation: Continuation::None,
            cells: Vec::new(),
        }
    }

    #[test]
    fn expands_characters_with_columns() {
        let cells = cells(&line("a界b"));
        assert_eq!(cells.len(), 3);
        assert_eq!(cells[1].text, "界");
        assert_eq!(cells[1].column, 1);
        assert_eq!(cells[1].width, 2);
        assert_eq!(cells[2].column, 3);
    }

    #[test]
    fn keeps_grapheme_clusters_together() {
        let cells = cells(&line("e\u{301}x"));
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0].text, "e\u{301}");
        assert_eq!(cells[0].width, 1);
        assert_eq!(cells[1].column, 1);
    }

    #[test]
    fn finds_characters_by_column() {
        let line = line("hello");
        assert_eq!(char_at_column(&line, 0).expect("char").ch, 'h');
        assert_eq!(char_at_column(&line, 4).expect("char").ch, 'o');
        assert_eq!(char_at_column(&line, 99).expect("char").ch, 'o');
    }

    #[test]
    fn moves_by_characters() {
        let line = line("a界b");
        assert_eq!(next_char_column(&line, 0), Some(1));
        assert_eq!(next_char_column(&line, 1), Some(3));
        assert_eq!(prev_char_column(&line, 3), Some(1));
        assert_eq!(clamp_column(&line, 99), 3);
    }

    #[test]
    fn word_motion_uses_keyboard_conventions() {
        let line = line("foo bar-baz qux");
        assert_eq!(next_word_start(&line, 0), 4);
        assert_eq!(next_word_start(&line, 4), 8);
        assert_eq!(next_word_start(&line, 8), 12);
        assert_eq!(word_end(&line, 0), 2);
        assert_eq!(word_end(&line, 4), 6);
        assert_eq!(word_start(&line, 6), 4);
        assert_eq!(word_start(&line, 8), 8);
    }
}
