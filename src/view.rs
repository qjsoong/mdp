//! Drawing rendered content.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::coords;
use crate::document::Document;
use crate::search::Search;
use crate::selection::{Point, Selection};
use crate::theme::Theme;

/// Scroll and cursor state of the content area.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    /// Index of the first visible line.
    pub scroll: usize,
    /// Horizontal scroll offset in display columns.
    pub hscroll: usize,
    /// Cursor position in the content.
    pub cursor: Point,
}

impl Default for View {
    fn default() -> Self {
        Self {
            scroll: 0,
            hscroll: 0,
            cursor: Point::new(0, 0),
        }
    }
}

impl View {
    /// Scrolls the viewport, keeping the offset in range.
    pub fn scroll_by(&mut self, delta: i64, line_count: usize, height: usize) {
        let max = line_count.saturating_sub(height);
        let target = self.scroll as i64 + delta;
        self.scroll = target.clamp(0, max as i64) as usize;
    }

    /// Moves the viewport so the cursor is visible.
    pub fn ensure_visible(&mut self, width: usize, height: usize, document: &Document) {
        if self.cursor.line < self.scroll {
            self.scroll = self.cursor.line;
        } else if height > 0 && self.cursor.line >= self.scroll + height {
            self.scroll = self.cursor.line + 1 - height;
        }
        let line_width = document
            .lines()
            .get(self.cursor.line)
            .map_or(0, coords::line_width);
        let maximum = line_width.saturating_sub(width.max(1));
        self.hscroll = self.hscroll.min(maximum);
        if self.cursor.col < self.hscroll {
            self.hscroll = self.cursor.col;
        } else if width > 0 && self.cursor.col >= self.hscroll + width {
            self.hscroll = self.cursor.col + 1 - width;
        }
    }

    /// Clamps the cursor to the current document.
    pub fn clamp(&mut self, document: &Document, height: usize) {
        let last = document.line_count().saturating_sub(1);
        self.cursor.line = self.cursor.line.min(last);
        let line_width = document
            .lines()
            .get(self.cursor.line)
            .map_or(0, coords::line_width);
        self.cursor.col = self.cursor.col.min(line_width.saturating_sub(1));
        self.scroll = self.scroll.min(last);
        if self.scroll + height > last + 1 {
            self.scroll = (last + 1).saturating_sub(height);
        }
    }
}

/// Everything the content area needs to draw itself.
pub struct Scene<'a> {
    /// The open document.
    pub document: &'a Document,
    /// Scroll and cursor state.
    pub view: &'a View,
    /// Active search with its highlights.
    pub search: &'a Search,
    /// Active selection.
    pub selection: Option<&'a Selection>,
    /// Active theme.
    pub theme: &'a Theme,
    /// Cursor to highlight, when it should be visible.
    pub cursor: Option<Point>,
}

/// Draws the content area, including the scrollbar in the rightmost column.
pub fn draw(buf: &mut Buffer, area: Rect, scene: &Scene<'_>) {
    let document = scene.document;
    let view = scene.view;
    let search = scene.search;
    let selection = scene.selection;
    let theme = scene.theme;
    let cursor = scene.cursor;
    if area.width == 0 || area.height == 0 {
        return;
    }
    let gutter = u16::from(area.width > 1);
    let width = area.width.saturating_sub(gutter);
    let height = usize::from(area.height);
    let lines = document.lines();
    let code_span = |line: &mdk_render::term::Line| {
        (line.kind == mdk_render::term::LineKind::Code)
            .then(|| line.spans.iter().position(|span| !span.source.is_none()))?
    };
    let _ = &code_span;
    if lines.is_empty() {
        buf.set_stringn(
            area.x,
            area.y,
            "(empty document)",
            16.min(usize::from(width)),
            Style::default().fg(theme.muted),
        );
        return;
    }
    for row in 0..height {
        let index = view.scroll + row;
        let Some(line) = lines.get(index) else {
            break;
        };
        let y = area.y + row as u16;
        let mut cursor_drawn = false;
        for cell in coords::cells(line) {
            if cell.width == 0 || cell.column < view.hscroll {
                continue;
            }
            let screen = cell.column - view.hscroll;
            if screen >= usize::from(width) {
                break;
            }
            let x = area.x + screen as u16;
            let mut style = theme.span_style(line, &line.spans[cell.span]);
            if line.kind == mdk_render::term::LineKind::Code
                && let Some(span_index) = code_span(line)
                && span_index == cell.span
                && let Some(ranges) = document.highlights.get(index)
                && let Some((_, color)) = ranges
                    .iter()
                    .find(|(range, _)| cell.byte >= range.start && cell.byte < range.end)
            {
                style = style.fg(*color);
            }
            style = apply_search(style, search, index, &cell, theme);
            if let Some(selection) = selection
                && selection.covers(index, cell.column, cell.width)
            {
                style = style.patch(theme.selection);
            }
            if cursor == Some(Point::new(index, cell.column)) {
                style = style.patch(theme.cursor);
                cursor_drawn = true;
            }
            buf.set_stringn(x, y, &cell.text, cell.width, style);
        }
        if let Some(point) = cursor
            && point.line == index
            && !cursor_drawn
        {
            let screen = point.col.saturating_sub(view.hscroll);
            if screen < usize::from(width) {
                buf.set_stringn(area.x + screen as u16, y, " ", 1, theme.cursor);
            }
        }
    }
    if gutter > 0 {
        draw_scrollbar(buf, area, document.line_count(), view.scroll, height, theme);
    }
}

fn apply_search(
    mut style: Style,
    search: &Search,
    line: usize,
    cell: &coords::Cell,
    theme: &Theme,
) -> Style {
    if search.hits.is_empty() {
        return style;
    }
    let active = search.current_hit();
    for hit in search.hits_on_line(line) {
        if cell.column < hit.end && cell.column + cell.width > hit.start {
            style = style.patch(theme.search);
            if Some(*hit) == active {
                style = style.patch(theme.search_active);
            }
        }
    }
    style
}

fn draw_scrollbar(
    buf: &mut Buffer,
    area: Rect,
    line_count: usize,
    scroll: usize,
    height: usize,
    theme: &Theme,
) {
    let x = area.x + area.width - 1;
    if line_count <= height || height == 0 {
        return;
    }
    let track = height;
    let thumb = (track * track / line_count).max(1);
    let position = if line_count > track {
        (scroll * (track - thumb)) / (line_count - track)
    } else {
        0
    };
    for row in 0..track {
        let ch = if row >= position && row < position + thumb {
            "█"
        } else {
            "│"
        };
        let style = if ch == "█" {
            Style::default().fg(theme.scrollbar)
        } else {
            Style::default().fg(theme.muted)
        };
        buf.set_stringn(x, area.y + row as u16, ch, 1, style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_by_stays_in_range() {
        let mut view = View::default();
        view.scroll_by(10, 100, 20);
        assert_eq!(view.scroll, 10);
        view.scroll_by(1000, 100, 20);
        assert_eq!(view.scroll, 80);
        view.scroll_by(-1000, 100, 20);
        assert_eq!(view.scroll, 0);
    }
}
