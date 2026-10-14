//! Searching rendered text.

use mdk_render::term::Line;

use crate::coords;

/// One search hit, in display coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Hit {
    /// Rendered line index.
    pub line: usize,
    /// First display column of the match.
    pub start: usize,
    /// Display column just past the match.
    pub end: usize,
}

/// Search state: the query, its hits and the current hit.
#[derive(Debug, Clone, Default)]
pub struct Search {
    /// Current query.
    pub query: String,
    /// Hits in reading order.
    pub hits: Vec<Hit>,
    /// Index into `hits`.
    pub current: usize,
}

impl Search {
    /// Recomputes the hits for a query.
    pub fn update(&mut self, query: &str, lines: &[Line], case_sensitive: bool) {
        self.query = query.to_string();
        self.hits = find(lines, query, case_sensitive);
        self.current = 0;
    }

    /// Clears the query and its highlights.
    pub fn clear(&mut self) {
        self.query.clear();
        self.hits.clear();
        self.current = 0;
    }

    /// Whether a query is active.
    #[must_use]
    pub fn is_active(&self) -> bool {
        !self.query.is_empty()
    }

    /// The current hit.
    #[must_use]
    pub fn current_hit(&self) -> Option<Hit> {
        self.hits.get(self.current).copied()
    }

    /// The hits on one line, in order.
    #[must_use]
    pub fn hits_on_line(&self, line: usize) -> &[Hit] {
        let start = self.hits.partition_point(|hit| hit.line < line);
        let end = self.hits.partition_point(|hit| hit.line <= line);
        &self.hits[start..end]
    }

    /// Moves to the next hit at or after `from`, wrapping around.
    #[must_use]
    pub fn seek_forward(&mut self, from: crate::selection::Point) -> Option<Hit> {
        if self.hits.is_empty() {
            return None;
        }
        let index = self
            .hits
            .partition_point(|hit| (hit.line, hit.start) < (from.line, from.col));
        self.current = if index >= self.hits.len() { 0 } else { index };
        self.hits.get(self.current).copied()
    }

    /// Moves to the previous hit before `from`, wrapping around.
    #[must_use]
    pub fn seek_backward(&mut self, from: crate::selection::Point) -> Option<Hit> {
        if self.hits.is_empty() {
            return None;
        }
        let index = self
            .hits
            .partition_point(|hit| (hit.line, hit.start) < (from.line, from.col));
        self.current = if index == 0 {
            self.hits.len() - 1
        } else {
            index - 1
        };
        self.hits.get(self.current).copied()
    }
}

/// Finds every occurrence of `query` in the rendered lines.
///
/// When `case_sensitive` is false the search is smart case: a query with an
/// upper-case letter is case sensitive, an all-lower-case query is not.
#[must_use]
pub fn find(lines: &[Line], query: &str, case_sensitive: bool) -> Vec<Hit> {
    if query.is_empty() {
        return Vec::new();
    }
    let sensitive = case_sensitive || query.chars().any(char::is_uppercase);
    let needle: Vec<char> = query.chars().collect();
    let mut hits = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let text: Vec<char> = line.text().chars().collect();
        if needle.len() > text.len() {
            continue;
        }
        let columns = char_columns(line);
        for start in 0..=text.len() - needle.len() {
            if !matches_at(&text, &needle, start, sensitive) {
                continue;
            }
            let end = start + needle.len() - 1;
            hits.push(Hit {
                line: index,
                start: columns[start],
                end: columns[end] + coords::char_width(text[end]),
            });
        }
    }
    hits
}

fn matches_at(text: &[char], needle: &[char], at: usize, sensitive: bool) -> bool {
    needle.iter().enumerate().all(|(offset, expected)| {
        let found = text[at + offset];
        if sensitive {
            found == *expected
        } else {
            found.to_lowercase().eq(expected.to_lowercase())
        }
    })
}

fn char_columns(line: &Line) -> Vec<usize> {
    let mut columns = Vec::new();
    let mut column = 0usize;
    for span in &line.spans {
        for ch in span.text.chars() {
            columns.push(column);
            column += coords::char_width(ch);
        }
    }
    columns
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdk_core::Dialect;
    use mdk_render::{TermOptions, render_terminal_document};

    fn lines(markdown: &str) -> Vec<Line> {
        render_terminal_document(markdown, Dialect::Gfm, &TermOptions::default()).lines
    }

    fn point(line: usize, col: usize) -> crate::selection::Point {
        crate::selection::Point::new(line, col)
    }

    #[test]
    fn finds_all_occurrences_with_columns() {
        let lines = lines("alpha beta alpha\n");
        let hits = find(&lines, "alpha", false);
        assert_eq!(hits.len(), 2);
        assert_eq!(
            hits[0],
            Hit {
                line: 0,
                start: 0,
                end: 5
            }
        );
        assert_eq!(
            hits[1],
            Hit {
                line: 0,
                start: 11,
                end: 16
            }
        );
    }

    #[test]
    fn smart_case_is_insensitive_for_lower_case_queries() {
        let lines = lines("Hello HELLO hello\n");
        assert_eq!(find(&lines, "hello", false).len(), 3);
        assert_eq!(find(&lines, "Hello", false).len(), 1);
        assert_eq!(find(&lines, "hello", true).len(), 1);
    }

    #[test]
    fn seeks_forward_and_wraps() {
        let lines = lines("one two one two\n");
        let mut search = Search::default();
        search.update("one", &lines, false);
        assert_eq!(search.hits.len(), 2);
        let hit = search.seek_forward(point(0, 0)).expect("hit");
        assert_eq!(hit.start, 0);
        let hit = search.seek_forward(point(0, 1)).expect("hit");
        assert_eq!(hit.start, 8);
        let hit = search.seek_forward(point(0, 20)).expect("wrapped");
        assert_eq!(hit.start, 0);
        let hit = search.seek_backward(point(0, 8)).expect("hit");
        assert_eq!(hit.start, 0);
        let hit = search.seek_backward(point(0, 0)).expect("wrapped");
        assert_eq!(hit.start, 8);
    }

    #[test]
    fn tracks_hits_per_line() {
        let lines = lines("alpha\n\nbeta alpha\n");
        let mut search = Search::default();
        search.update("alpha", &lines, false);
        assert_eq!(search.hits_on_line(0).len(), 1);
        assert_eq!(search.hits_on_line(1).len(), 0);
        assert_eq!(search.hits_on_line(2).len(), 1);
    }

    #[test]
    fn matches_across_wide_characters() {
        let lines = lines("世界 hello 世界\n");
        let hits = find(&lines, "hello", false);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].start, 5);
        assert_eq!(hits[0].end, 10);
    }
}
