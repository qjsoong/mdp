//! The status bar, help popup and outline popup.

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Mode};
use crate::theme::Theme;

/// Draws the bottom status bar.
pub fn draw_status(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let buf = frame.buffer_mut();
    for x in area.x..area.right() {
        buf.set_stringn(x, area.y, " ", 1, theme.status);
    }
    let mut left: Vec<Span<'_>> = Vec::new();
    left.push(Span::styled(
        format!(" {} ", app.document.title()),
        theme.status,
    ));
    let percent = app.document.percent_at(app.view.cursor.line);
    left.push(Span::styled(
        format!(
            "{}%  {}:{}  ",
            percent,
            app.view.cursor.line + 1,
            app.view.cursor.col + 1
        ),
        theme.status,
    ));
    if app.mode == Mode::Search {
        left.push(Span::styled("search: ", theme.status));
        left.push(Span::styled(
            format!("/{}", app.search_input),
            theme.status_message,
        ));
        if !app.search.hits.is_empty() {
            left.push(Span::styled(
                format!("  {}/{}", app.search.current + 1, app.search.hits.len()),
                theme.status,
            ));
        }
        left.push(Span::styled("  Enter jump  Esc cancel", theme.status));
    } else if let Some(selection) = &app.selection
        && !selection.is_empty()
    {
        let lines = selection.lines().len();
        left.push(Span::styled(
            format!("selection: {lines} line(s)  y copy  Y source  Esc cancel"),
            theme.status,
        ));
    } else if let Some(link) = app.document.link_at(app.view.cursor) {
        left.push(Span::styled(
            format!("→ {} ", link.url),
            theme.status_message,
        ));
    } else if let Some(heading) = app.document.heading_before(app.view.cursor.line) {
        left.push(Span::styled(format!("# {} ", heading.text), theme.status));
    }
    let right_text = if app.mode == Mode::Search {
        String::new()
    } else {
        app.message()
            .map(str::to_string)
            .unwrap_or_else(|| "? help  t outline  / search  q quit".to_string())
    };
    let style = if app.message().is_some() {
        theme.status_message
    } else {
        theme.status
    };
    if !right_text.is_empty() {
        let right = Paragraph::new(Line::from(Span::styled(format!(" {right_text} "), style)))
            .alignment(Alignment::Right);
        frame.render_widget(right, area);
    }
    let right_width = if right_text.is_empty() {
        0
    } else {
        (right_text.width() + 2) as u16
    };
    let left_area = Rect {
        width: area.width.saturating_sub(right_width),
        ..area
    };
    frame.render_widget(
        Paragraph::new(Line::from(left)).style(theme.status),
        left_area,
    );
}

/// Draws the help popup.
pub fn draw_help(frame: &mut Frame, area: Rect, theme: &Theme) {
    let lines = help_lines(theme);
    let width = lines.iter().map(|line| line.width()).max().unwrap_or(40) as u16 + 4;
    let height = lines.len() as u16 + 2;
    let rect = centered(area, width, height);
    frame.render_widget(Clear, rect);
    let block = popup_block(&format!(" Keys · {} theme ", theme.name), theme);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    frame.render_widget(Paragraph::new(lines), inner);
}

/// Draws the outline popup.
pub fn draw_toc(frame: &mut Frame, area: Rect, app: &App) {
    let headings = &app.document.headings;
    let height = (headings.len() as u16 + 2)
        .min(area.height.saturating_sub(2))
        .max(3);
    let width = 64.min(area.width.saturating_sub(4)).max(16);
    let rect = centered(area, width, height);
    frame.render_widget(Clear, rect);
    let block = popup_block(" Outline ", &app.theme);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let visible = usize::from(inner.height);
    let start = if app.toc_selected >= visible {
        app.toc_selected + 1 - visible
    } else {
        0
    };
    for (row, heading) in headings.iter().skip(start).take(visible).enumerate() {
        let index = start + row;
        let selected = index == app.toc_selected;
        let indent = "  ".repeat(usize::from(heading.level.saturating_sub(1)));
        let marker = if selected { "▸ " } else { "  " };
        let text = format!("{indent}{marker}{}", heading.text);
        let style = if selected {
            app.theme.popup_selected
        } else {
            Style::default().fg(app.theme.text)
        };
        let line = Line::from(Span::styled(truncate(&text, inner.width), style));
        frame.render_widget(
            Paragraph::new(line),
            Rect {
                x: inner.x,
                y: inner.y + row as u16,
                width: inner.width,
                height: 1,
            },
        );
    }
}

fn popup_block(title: &str, theme: &Theme) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(theme.popup_border)
        .style(Style::default().bg(ratatui::style::Color::Reset))
        .title(Span::styled(title.to_string(), theme.popup_border))
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

fn truncate(text: &str, width: u16) -> String {
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + ch_width > usize::from(width) {
            break;
        }
        out.push(ch);
        used += ch_width;
    }
    out
}

fn help_lines(theme: &Theme) -> Vec<Line<'static>> {
    const SECTIONS: &[(&str, &[(&str, &str)])] = &[
        (
            "Move",
            &[
                ("j / k", "line down / up"),
                ("h / l", "character left / right"),
                ("Ctrl-d / Ctrl-u", "half page down / up"),
                ("PgDn / PgUp, Space", "page down / up"),
                ("g / G", "top / bottom"),
                ("w / b / e", "word forward / back / end"),
                ("0 / $", "start / end of line"),
                ("{ / }", "previous / next block"),
                ("Tab / Shift-Tab", "next / previous heading"),
            ],
        ),
        (
            "Copy",
            &[
                ("v", "start selecting; move, then y"),
                ("y", "copy selection, or the block at the cursor"),
                ("Y", "copy the raw Markdown source instead"),
                ("drag", "mouse selection, copied on release"),
            ],
        ),
        (
            "Find",
            &[
                ("/", "search; n / N for next / previous"),
                ("Esc", "clear search or selection"),
            ],
        ),
        (
            "Other",
            &[
                ("t", "outline"),
                ("o / Enter", "open the link under the cursor"),
                ("r / R", "reload from disk"),
                ("? ", "this help"),
                ("q", "quit"),
            ],
        ),
    ];
    let mut lines = Vec::new();
    for (section, entries) in SECTIONS {
        if !lines.is_empty() {
            lines.push(Line::default());
        }
        lines.push(Line::from(Span::styled(*section, theme.search_active)));
        for (key, description) in *entries {
            lines.push(Line::from(vec![
                Span::styled(format!("  {key:<18}"), theme.link),
                Span::styled((*description).to_string(), theme.text),
            ]));
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_respects_display_width() {
        assert_eq!(truncate("abcdef", 3), "abc");
        assert_eq!(truncate("世界", 3), "世");
        assert_eq!(truncate("abc", 10), "abc");
    }

    #[test]
    fn centered_fits_inside_the_area() {
        let area = Rect::new(0, 0, 20, 10);
        let rect = centered(area, 100, 100);
        assert_eq!(rect, area);
        let rect = centered(area, 10, 4);
        assert_eq!(rect.x, 5);
        assert_eq!(rect.y, 3);
    }
}
