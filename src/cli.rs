//! Command line interface.

use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use mdk_core::Dialect;

/// Markdown dialect understood by the reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum DialectChoice {
    /// GitHub Flavored Markdown: tables, task lists, strikethrough, autolinks.
    #[default]
    Gfm,
    /// CommonMark with no extensions.
    Commonmark,
}

impl From<DialectChoice> for Dialect {
    fn from(choice: DialectChoice) -> Self {
        match choice {
            DialectChoice::Gfm => Dialect::Gfm,
            DialectChoice::Commonmark => Dialect::Commonmark,
        }
    }
}

/// A terminal Markdown reader with clean copying.
///
/// Rendered content keeps its formatting; selecting text and copying it yields
/// the Markdown source it came from, without table borders, list bullets or
/// code indentation added by the renderer.
#[derive(Debug, Parser)]
#[command(name = "mdp", version, about, long_about = None)]
pub struct Cli {
    /// Markdown file to read; `-` reads standard input
    #[arg(value_name = "FILE")]
    pub file: Option<String>,

    /// Print the rendered document and exit instead of opening the reader
    #[arg(short, long)]
    pub print: bool,

    /// Wrap width; defaults to the terminal width
    #[arg(short, long, value_name = "COLUMNS")]
    pub width: Option<usize>,

    /// Markdown dialect
    #[arg(long, value_enum, default_value_t)]
    pub dialect: DialectChoice,

    /// Configuration file to use instead of the default location
    #[arg(long, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Disable colour
    #[arg(long)]
    pub no_color: bool,

    /// Write a default configuration file and exit
    #[arg(long)]
    pub init: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_file_and_flags() {
        let cli = Cli::try_parse_from(["mdp", "--print", "-w", "72", "README.md"]).expect("parse");
        assert_eq!(cli.file.as_deref(), Some("README.md"));
        assert!(cli.print);
        assert_eq!(cli.width, Some(72));
    }

    #[test]
    fn dialect_defaults_to_gfm() {
        let cli = Cli::try_parse_from(["mdp"]).expect("parse");
        assert_eq!(cli.dialect, DialectChoice::Gfm);
        assert_eq!(Dialect::from(cli.dialect), Dialect::Gfm);
    }

    #[test]
    fn rejects_unknown_dialect() {
        assert!(Cli::try_parse_from(["mdp", "--dialect", "nope"]).is_err());
    }
}
