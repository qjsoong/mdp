//! Colour themes and mapping from rendered Markdown styles.

use mdk_render::term::{Color as MarkdownColor, Line, LineKind, SourceMap, Span};
use ratatui::style::{Color, Modifier, Style};

use crate::config::ThemeChoice;

/// A palette for the reader chrome and for rendered Markdown.
#[derive(Debug, Clone)]
pub struct Theme {
    /// Theme name, shown in the help popup.
    pub name: &'static str,
    /// Default body text.
    pub text: Color,
    /// Secondary text: quotes, table borders, URLs, raw HTML.
    pub muted: Color,
    /// Heading colours by level (index 0 is level 1).
    pub headings: [Color; 6],
    /// Code blocks and inline code.
    pub code: Color,
    /// Linked text.
    pub link: Color,
    /// The URL next to a link.
    pub link_url: Color,
    /// Bullets and list markers.
    pub marker: Color,
    /// Completed task list items.
    pub success: Color,
    /// Highlighted background inside Markdown.
    pub highlight: Color,
    /// Footnote references and maths.
    pub accent: Color,
    /// Text selection from the mouse or visual mode.
    pub selection: Style,
    /// The cursor cell in normal mode.
    pub cursor: Style,
    /// Search matches.
    pub search: Style,
    /// The search match the cursor is on.
    pub search_active: Style,
    /// The status bar.
    pub status: Style,
    /// The status bar message highlight.
    pub status_message: Style,
    /// Popup border.
    pub popup_border: Style,
    /// Selected row in a popup.
    pub popup_selected: Style,
    /// Scrollbar thumb.
    pub scrollbar: Color,
}

impl Theme {
    /// A palette for dark terminal backgrounds.
    #[must_use]
    pub fn dark() -> Self {
        Self {
            name: "dark",
            text: Color::Reset,
            muted: Color::DarkGray,
            headings: [
                Color::Cyan,
                Color::LightBlue,
                Color::LightCyan,
                Color::LightYellow,
                Color::LightYellow,
                Color::LightYellow,
            ],
            code: Color::LightYellow,
            link: Color::LightBlue,
            link_url: Color::DarkGray,
            marker: Color::Blue,
            success: Color::LightGreen,
            highlight: Color::Yellow,
            accent: Color::LightMagenta,
            selection: Style::default()
                .bg(Color::LightBlue)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
            cursor: Style::default().add_modifier(Modifier::REVERSED),
            search: Style::default().bg(Color::Yellow).fg(Color::Black),
            search_active: Style::default()
                .bg(Color::LightGreen)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
            status: Style::default().bg(Color::DarkGray).fg(Color::White),
            status_message: Style::default()
                .bg(Color::LightGreen)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
            popup_border: Style::default().fg(Color::DarkGray),
            popup_selected: Style::default()
                .bg(Color::LightBlue)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
            scrollbar: Color::DarkGray,
        }
    }

    /// A palette for light terminal backgrounds.
    #[must_use]
    pub fn light() -> Self {
        Self {
            name: "light",
            text: Color::Reset,
            muted: Color::Gray,
            headings: [
                Color::Blue,
                Color::Blue,
                Color::Cyan,
                Color::Magenta,
                Color::Magenta,
                Color::Magenta,
            ],
            code: Color::Magenta,
            link: Color::Blue,
            link_url: Color::Gray,
            marker: Color::Blue,
            success: Color::Green,
            highlight: Color::LightYellow,
            accent: Color::Magenta,
            selection: Style::default()
                .bg(Color::LightBlue)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
            cursor: Style::default().add_modifier(Modifier::REVERSED),
            search: Style::default().bg(Color::LightYellow).fg(Color::Black),
            search_active: Style::default()
                .bg(Color::LightGreen)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
            status: Style::default().bg(Color::Gray).fg(Color::Black),
            status_message: Style::default()
                .bg(Color::LightGreen)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
            popup_border: Style::default().fg(Color::Gray),
            popup_selected: Style::default()
                .bg(Color::LightBlue)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
            scrollbar: Color::Gray,
        }
    }

    /// A palette without colour, for `NO_COLOR` or monochrome terminals.
    #[must_use]
    pub fn plain() -> Self {
        Self {
            name: "none",
            text: Color::Reset,
            muted: Color::Reset,
            headings: [Color::Reset; 6],
            code: Color::Reset,
            link: Color::Reset,
            link_url: Color::Reset,
            marker: Color::Reset,
            success: Color::Reset,
            highlight: Color::Reset,
            accent: Color::Reset,
            selection: Style::default().add_modifier(Modifier::REVERSED),
            cursor: Style::default().add_modifier(Modifier::REVERSED),
            search: Style::default().add_modifier(Modifier::UNDERLINED),
            search_active: Style::default().add_modifier(Modifier::UNDERLINED | Modifier::BOLD),
            status: Style::default().add_modifier(Modifier::REVERSED),
            status_message: Style::default().add_modifier(Modifier::REVERSED),
            popup_border: Style::default(),
            popup_selected: Style::default().add_modifier(Modifier::REVERSED),
            scrollbar: Color::Reset,
        }
    }

    /// Resolves the configured choice, detecting the background when needed.
    #[must_use]
    pub fn resolve(choice: ThemeChoice, no_color: bool) -> Self {
        if no_color || std::env::var_os("NO_COLOR").is_some() {
            return Self::plain();
        }
        match choice {
            ThemeChoice::Dark => Self::dark(),
            ThemeChoice::Light => Self::light(),
            ThemeChoice::None => Self::plain(),
            ThemeChoice::Auto => {
                if terminal_background_is_light() {
                    Self::light()
                } else {
                    Self::dark()
                }
            }
        }
    }

    /// The style of one rendered span, with semantic colours for its role.
    #[must_use]
    pub fn span_style(&self, line: &Line, span: &Span) -> Style {
        let markdown = &span.style;
        let mut style = Style::default();
        if markdown.bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        if markdown.dim {
            style = style.add_modifier(Modifier::DIM);
        }
        if markdown.italic {
            style = style.add_modifier(Modifier::ITALIC);
        }
        if markdown.underline {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        if markdown.strike {
            style = style.add_modifier(Modifier::CROSSED_OUT);
        }
        if let Some(background) = markdown.bg {
            style = style.bg(match background {
                MarkdownColor::Yellow => self.highlight,
                _ => Color::Reset,
            });
        }
        style.fg(self.role_color(line, span))
    }

    /// The foreground colour of a span inside a line.
    fn role_color(&self, line: &Line, span: &Span) -> Color {
        match line.kind {
            LineKind::Heading(level) => {
                return self.headings[usize::from(level.saturating_sub(1).min(5))];
            }
            LineKind::Code => return self.code,
            LineKind::Rule | LineKind::TableBorder => return self.muted,
            LineKind::Html => return self.muted,
            LineKind::TableRow if line.cells.iter().any(|cell| cell.header) => {
                return self.text;
            }
            _ => {}
        }
        match span.style.fg {
            Some(MarkdownColor::BrightBlack) if span.style.dim => self.link_url,
            Some(MarkdownColor::BrightBlack) => self.muted,
            Some(MarkdownColor::BrightYellow) => self.code,
            Some(MarkdownColor::BrightBlue) => self.link,
            Some(MarkdownColor::Magenta) => self.accent,
            Some(MarkdownColor::Cyan) => self.accent,
            Some(MarkdownColor::Green) => self.success,
            _ if is_generated_marker(span) => self.marker,
            _ => self.text,
        }
    }
}

/// Whether a span is a generated list marker or block quote gutter.
fn is_generated_marker(span: &Span) -> bool {
    if !matches!(span.source, SourceMap::None) {
        return false;
    }
    let text = span.text.as_str();
    (span.style.dim && matches!(text, "│ " | "> ")) || matches!(text, "•" | "• ")
}

/// Whether the terminal reports a light background through `COLORFGBG`.
fn terminal_background_is_light() -> bool {
    let Some(value) = std::env::var_os("COLORFGBG") else {
        return false;
    };
    let value = value.to_string_lossy();
    let Some(background) = value.rsplit(';').next() else {
        return false;
    };
    matches!(background.trim().parse::<u8>(), Ok(7 | 15))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdk_render::term::{Continuation, SourceMap, Style as MarkdownStyle};

    fn line(kind: LineKind, spans: Vec<Span>) -> Line {
        Line {
            spans,
            kind,
            block: Some(0),
            continuation: Continuation::None,
            cells: Vec::new(),
        }
    }

    #[test]
    fn headings_use_their_level_colour() {
        let theme = Theme::dark();
        let span = Span::new(MarkdownStyle::default(), "Title");
        let style = theme.span_style(&line(LineKind::Heading(2), vec![span.clone()]), &span);
        assert_eq!(style.fg, Some(Color::LightBlue));
    }

    #[test]
    fn code_uses_the_code_colour() {
        let theme = Theme::dark();
        let span = Span::mapped(
            MarkdownStyle::default(),
            "let x = 1;",
            SourceMap::Verbatim(4),
        );
        let style = theme.span_style(&line(LineKind::Code, vec![span.clone()]), &span);
        assert_eq!(style.fg, Some(Color::LightYellow));
    }

    #[test]
    fn markdown_attributes_survive() {
        let theme = Theme::plain();
        let span = Span::new(MarkdownStyle::default().bold(), "bold");
        let style = theme.span_style(&line(LineKind::Text, vec![span.clone()]), &span);
        assert!(style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(style.fg, Some(Color::Reset));
    }

    #[test]
    fn bullets_use_the_marker_colour() {
        let theme = Theme::dark();
        let span = Span::plain("•");
        let style = theme.span_style(&line(LineKind::Text, vec![span.clone()]), &span);
        assert_eq!(style.fg, Some(Color::Blue));
    }
}
