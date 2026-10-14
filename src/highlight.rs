//! Syntax highlighting for code blocks.

use std::ops::Range;

use ratatui::style::Color;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

use crate::config::ThemeChoice;

/// A syntax highlighter for code blocks.
pub struct Highlighter {
    syntaxes: SyntaxSet,
    theme: syntect::highlighting::Theme,
}

impl Highlighter {
    /// Loads the bundled syntax and theme sets.
    ///
    /// Returns `None` when the theme named for the palette is missing.
    #[must_use]
    pub fn new(choice: ThemeChoice) -> Option<Self> {
        let syntaxes = SyntaxSet::load_defaults_newlines();
        let themes = ThemeSet::load_defaults();
        let name = match choice {
            ThemeChoice::Light => "InspiredGitHub",
            _ => "base16-ocean.dark",
        };
        let theme = themes.themes.get(name)?.clone();
        Some(Self { syntaxes, theme })
    }

    /// Highlights the lines of one code block.
    ///
    /// Each returned vector holds byte ranges into the corresponding input
    /// line and the colour of that range. Ranges cover the whole line.
    #[must_use]
    pub fn highlight(
        &self,
        language: Option<&str>,
        lines: &[&str],
    ) -> Vec<Vec<(Range<usize>, Color)>> {
        let syntax = language
            .and_then(|token| self.syntaxes.find_syntax_by_token(token))
            .unwrap_or_else(|| self.syntaxes.find_syntax_plain_text());
        let mut highlighter = HighlightLines::new(syntax, &self.theme);
        let mut out = Vec::with_capacity(lines.len());
        for line in lines {
            let with_newline = format!("{line}\n");
            let mut ranges = Vec::new();
            if let Ok(regions) = highlighter.highlight_line(&with_newline, &self.syntaxes) {
                let mut offset = 0usize;
                for (style, text) in regions {
                    let end = (offset + text.len()).min(line.len());
                    if end > offset {
                        ranges.push((offset..end, to_color(style.foreground)));
                    }
                    offset += text.len();
                }
            }
            out.push(ranges);
        }
        out
    }
}

fn to_color(color: syntect::highlighting::Color) -> Color {
    Color::Rgb(color.r, color.g, color.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_keywords_in_rust() {
        let Some(highlighter) = Highlighter::new(ThemeChoice::Dark) else {
            return;
        };
        let lines = vec!["fn main() {", "    let x = 1;", "}"];
        let styled = highlighter.highlight(Some("rust"), &lines);
        assert_eq!(styled.len(), 3);
        for (line, ranges) in lines.iter().zip(&styled) {
            let total: usize = ranges.iter().map(|(range, _)| range.len()).sum();
            assert_eq!(total, line.len(), "ranges must cover {line:?}");
        }
        let colors: Vec<Color> = styled[0].iter().map(|(_, color)| *color).collect();
        assert!(
            colors.iter().any(|color| *color != colors[0]),
            "`fn` should be coloured differently from the rest"
        );
    }

    #[test]
    fn unknown_language_falls_back_to_plain_text() {
        let Some(highlighter) = Highlighter::new(ThemeChoice::Dark) else {
            return;
        };
        let styled = highlighter.highlight(Some("not-a-language"), &["hello"]);
        assert_eq!(styled.len(), 1);
    }

    #[test]
    fn handles_empty_lines_and_missing_language() {
        let Some(highlighter) = Highlighter::new(ThemeChoice::Dark) else {
            return;
        };
        let styled = highlighter.highlight(None, &["", "text"]);
        assert_eq!(styled.len(), 2);
        assert!(styled[0].iter().all(|(range, _)| range.is_empty()));
    }
}
