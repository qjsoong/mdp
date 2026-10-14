//! Loading and rendering a Markdown document.

use std::fs;
use std::io::{IsTerminal, Read};
use std::path::PathBuf;
use std::time::SystemTime;

use anyhow::{Context, Result};
use mdk_core::Dialect;
use mdk_render::term::{BlockKind, Line, LineKind, SourceMap};
use mdk_render::{TermOptions, render_terminal_document};

use crate::coords;
use crate::highlight::Highlighter;
use crate::index::{Index, LinkRef};
use crate::selection::Point;

/// The code text of a rendered code line, without its gutter.
fn code_text(line: &Line) -> &str {
    line.spans
        .iter()
        .find(|span| !span.source.is_none())
        .map_or("", |span| span.text.as_str())
}

/// The first source offset mapped on a rendered line.
fn first_source_offset(line: &Line) -> Option<usize> {
    line.spans.iter().find_map(|span| match &span.source {
        SourceMap::Verbatim(start) => Some(*start),
        SourceMap::PerChar(ranges) => ranges.iter().flatten().map(|range| range.start).min(),
        SourceMap::None => None,
    })
}

/// A heading and the rendered line it starts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// Heading level, 1 to 6.
    pub level: u8,
    /// Heading text without Markdown markers.
    pub text: String,
    /// Rendered line index where the heading starts.
    pub line: usize,
}

/// An opened Markdown file: source text, rendered content and indexes.
pub struct Document {
    /// Path the document was read from; `None` for standard input.
    pub path: Option<PathBuf>,
    /// Original Markdown text.
    pub source: String,
    /// Dialect used to parse the document.
    pub dialect: Dialect,
    /// Rendered content with source mapping.
    pub rendered: mdk_render::term::Document,
    /// Links and images in the document.
    pub index: Index,
    /// Headings, in document order.
    pub headings: Vec<Heading>,
    /// Width the content was rendered at.
    pub width: u16,
    /// Syntax colours per rendered line; empty for lines that are not code.
    pub highlights: Vec<Vec<(std::ops::Range<usize>, ratatui::style::Color)>>,
    modified: Option<SystemTime>,
}

impl Document {
    /// Opens a file, or standard input when `target` is `-` or empty and
    /// standard input is redirected.
    ///
    /// # Errors
    ///
    /// Returns an error when the input cannot be read or no input is given.
    pub fn open(target: Option<&str>, dialect: Dialect, width: u16) -> Result<Self> {
        let (path, source) = Self::read_source(target)?;
        let modified = path
            .as_ref()
            .and_then(|path| fs::metadata(path).and_then(|meta| meta.modified()).ok());
        let mut document = Self {
            path,
            source,
            dialect,
            rendered: mdk_render::term::Document::default(),
            index: Index::default(),
            headings: Vec::new(),
            width,
            highlights: Vec::new(),
            modified,
        };
        document.render(width);
        document.rebuild_index();
        Ok(document)
    }

    /// Creates a document from memory; used by tests.
    #[cfg(test)]
    pub(crate) fn from_source(markdown: &str, dialect: Dialect, width: u16) -> Self {
        let mut document = Self {
            path: None,
            source: markdown.to_string(),
            dialect,
            rendered: mdk_render::term::Document::default(),
            index: Index::default(),
            headings: Vec::new(),
            width,
            highlights: Vec::new(),
            modified: None,
        };
        document.render(width);
        document.rebuild_index();
        document
    }

    /// Reads a document from a file, or from standard input when `target` is
    /// `-` or when no file is given and standard input is redirected.
    ///
    /// # Errors
    ///
    /// Returns an error when the input cannot be read or no input is given.
    pub fn read_source(target: Option<&str>) -> Result<(Option<PathBuf>, String)> {
        let from_stdin = match target {
            Some("-") => true,
            None => !std::io::stdin().is_terminal(),
            Some(_) => false,
        };
        if from_stdin {
            let mut text = String::new();
            std::io::stdin()
                .read_to_string(&mut text)
                .context("cannot read standard input")?;
            return Ok((None, text));
        }
        let Some(target) = target else {
            anyhow::bail!("no input: pass a Markdown file, or pipe one to `mdp -`");
        };
        let path = PathBuf::from(target);
        let text =
            fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
        Ok((Some(path), text))
    }

    /// Renders the document at `width`, reusing the source text.
    pub fn render(&mut self, width: u16) {
        self.width = width.max(1);
        let options = TermOptions {
            width: usize::from(self.width),
            ..TermOptions::default()
        };
        self.rendered = render_terminal_document(&self.source, self.dialect, &options);
        self.highlights.clear();
        self.rebuild_headings();
    }

    /// Computes syntax colours for code blocks.
    pub fn highlight(&mut self, highlighter: Option<&Highlighter>) {
        self.highlights = vec![Vec::new(); self.rendered.lines.len()];
        let Some(highlighter) = highlighter else {
            return;
        };
        let mut index = 0;
        while index < self.rendered.lines.len() {
            if self.rendered.lines[index].kind != LineKind::Code {
                index += 1;
                continue;
            }
            let block = self.rendered.lines[index].block;
            let start = index;
            while index < self.rendered.lines.len()
                && self.rendered.lines[index].kind == LineKind::Code
                && self.rendered.lines[index].block == block
            {
                index += 1;
            }
            let texts: Vec<&str> = self.rendered.lines[start..index]
                .iter()
                .map(code_text)
                .collect();
            let language = first_source_offset(&self.rendered.lines[start])
                .and_then(|offset| self.index.code_language(offset));
            let styled = highlighter.highlight(language, &texts);
            for (offset, ranges) in styled.into_iter().enumerate() {
                if let Some(slot) = self.highlights.get_mut(start + offset) {
                    *slot = ranges;
                }
            }
        }
    }

    /// Rebuilds the link index from the current source text.
    pub fn rebuild_index(&mut self) {
        self.index = Index::build(&self.source, self.dialect);
    }

    /// Reloads the file when it changed on disk.
    ///
    /// # Errors
    ///
    /// Returns an error when the file exists but cannot be read.
    pub fn reload_if_changed(&mut self) -> Result<bool> {
        let Some(path) = self.path.clone() else {
            return Ok(false);
        };
        let stamp = fs::metadata(&path).and_then(|meta| meta.modified()).ok();
        if stamp.is_some() && stamp == self.modified {
            return Ok(false);
        }
        let text =
            fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
        self.modified = stamp;
        if text == self.source {
            return Ok(false);
        }
        self.source = text;
        let width = self.width;
        self.render(width);
        self.rebuild_index();
        Ok(true)
    }

    /// Reloads the file unconditionally.
    ///
    /// # Errors
    ///
    /// Returns an error when the file cannot be read.
    pub fn reload(&mut self) -> Result<bool> {
        let Some(path) = self.path.clone() else {
            return Ok(false);
        };
        let text =
            fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
        self.modified = fs::metadata(&path).and_then(|meta| meta.modified()).ok();
        if text == self.source {
            return Ok(false);
        }
        self.source = text;
        let width = self.width;
        self.render(width);
        self.rebuild_index();
        Ok(true)
    }

    /// The rendered lines.
    #[must_use]
    pub fn lines(&self) -> &[Line] {
        &self.rendered.lines
    }

    /// The number of rendered lines.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.rendered.lines.len()
    }

    /// The file name shown in the status bar.
    #[must_use]
    pub fn title(&self) -> String {
        self.path.as_ref().map_or_else(
            || "standard input".to_string(),
            |path| {
                path.file_name().map_or_else(
                    || path.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                )
            },
        )
    }

    /// The heading that most recently precedes `line`.
    #[must_use]
    pub fn heading_before(&self, line: usize) -> Option<&Heading> {
        self.headings
            .iter()
            .rev()
            .find(|heading| heading.line <= line)
    }

    /// Whether the source at `offset` is part of a link or image.
    #[must_use]
    pub fn link_at_offset(&self, offset: usize) -> Option<&LinkRef> {
        self.index.at(offset)
    }

    /// The link whose text or destination contains the cursor.
    #[must_use]
    pub fn link_at(&self, point: Point) -> Option<&LinkRef> {
        self.source_offset_at(point)
            .and_then(|offset| self.link_at_offset(offset))
    }

    /// The source offset displayed at a cursor position.
    #[must_use]
    pub fn source_offset_at(&self, point: Point) -> Option<usize> {
        let line = self.rendered.lines.get(point.line)?;
        coords::char_at_column(line, point.col)
            .and_then(|cell| cell.source)
            .map(|range| range.start)
    }

    /// The position of a rendered cursor, in percent of the document.
    #[must_use]
    pub fn percent_at(&self, line: usize) -> usize {
        let total = self.line_count().saturating_sub(1).max(1);
        (line.min(total) * 100) / total
    }

    fn rebuild_headings(&mut self) {
        self.headings.clear();
        let mut first_line = vec![None; self.rendered.blocks.len()];
        for (index, line) in self.rendered.lines.iter().enumerate() {
            if let Some(block) = line.block
                && first_line.get(block).is_some_and(Option::is_none)
            {
                first_line[block] = Some(index);
            }
        }
        for (block_index, block) in self.rendered.blocks.iter().enumerate() {
            if let BlockKind::Heading(level) = block.kind
                && let Some(line) = first_line.get(block_index).copied().flatten()
            {
                self.headings.push(Heading {
                    level,
                    text: self.rendered.lines[line].text(),
                    line,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(markdown: &str) -> Document {
        Document::from_source(markdown, Dialect::Gfm, 80)
    }

    #[test]
    fn collects_headings_in_order() {
        let document = document("# One\n\ntext\n\n## Two\n");
        let headings: Vec<(&str, u8)> = document
            .headings
            .iter()
            .map(|heading| (heading.text.as_str(), heading.level))
            .collect();
        assert_eq!(headings, vec![("One", 1), ("Two", 2)]);
        assert_eq!(document.headings[1].line, 4);
    }

    #[test]
    fn finds_the_heading_above_a_line() {
        let document = document("# One\n\ntext\n\n## Two\n\nmore\n");
        let heading = document.heading_before(2).expect("heading");
        assert_eq!(heading.text, "One");
        let heading = document.heading_before(6).expect("heading");
        assert_eq!(heading.text, "Two");
    }

    #[test]
    fn maps_offsets_to_links() {
        let document = document("[text](https://example.com)\n");
        let offset = document.source.find("text").expect("offset");
        let link = document.link_at_offset(offset).expect("link");
        assert_eq!(link.url, "https://example.com");
    }

    #[test]
    fn reports_percentages() {
        let document = document("a\n\nb\n\nc\n");
        assert_eq!(document.percent_at(0), 0);
        assert_eq!(document.percent_at(document.line_count() - 1), 100);
    }
}
