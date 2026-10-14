//! mdp — a terminal Markdown reader with clean copying.

mod app;
mod cli;
mod clipboard;
mod config;
mod coords;
mod document;
mod highlight;
mod index;
mod search;
mod selection;
mod theme;
mod ui;
mod view;

use std::io::{IsTerminal, Write};
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use mdk_core::Dialect;
use mdk_render::{TermOptions, render_terminal};
use ratatui::crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event};
use ratatui::crossterm::execute;

use crate::app::App;
use crate::cli::Cli;
use crate::config::Config;
use crate::document::Document;
use crate::theme::Theme;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let (config, warning) = Config::load(cli.config.as_deref());
    if let Some(warning) = &warning {
        eprintln!("mdp: {warning}");
    }
    if cli.init {
        let path = cli
            .config
            .clone()
            .or_else(Config::default_location)
            .context("cannot determine the configuration path")?;
        Config::write_default(&path)?;
        println!("wrote {}", path.display());
        return Ok(());
    }
    let dialect: Dialect = cli.dialect.into();
    if cli.print {
        return print_document(&cli, dialect);
    }
    let width = cli.width.unwrap_or(80).clamp(20, u16::MAX as usize) as u16;
    let target = default_target(cli.file.as_deref());
    let document = Document::open(target, dialect, width)?;
    let theme = Theme::resolve(config.theme, cli.no_color);
    run_reader(document, config, theme)
}

/// With no file and nothing on standard input, read `README.md` from the
/// current directory when it exists.
fn default_target(file: Option<&str>) -> Option<&str> {
    if file.is_some() || !std::io::stdin().is_terminal() {
        return file;
    }
    std::path::Path::new("README.md")
        .is_file()
        .then_some("README.md")
}

fn print_document(cli: &Cli, dialect: Dialect) -> Result<()> {
    let (_, source) = Document::read_source(cli.file.as_deref())?;
    let width = cli
        .width
        .or_else(|| {
            ratatui::crossterm::terminal::size()
                .ok()
                .map(|(columns, _)| usize::from(columns))
        })
        .unwrap_or(80)
        .max(20);
    let options = TermOptions {
        width,
        color: std::io::stdout().is_terminal() && !cli.no_color,
        ..TermOptions::default()
    };
    let output = render_terminal(&source, dialect, &options);
    let mut stdout = std::io::stdout();
    stdout
        .write_all(output.as_bytes())
        .context("cannot write to standard output")?;
    stdout.flush().context("cannot flush standard output")?;
    Ok(())
}

fn run_reader(document: Document, config: Config, theme: Theme) -> Result<()> {
    let mut terminal = ratatui::init();
    let mouse = config.mouse;
    if mouse {
        let _ = execute!(std::io::stdout(), EnableMouseCapture);
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = execute!(std::io::stdout(), DisableMouseCapture);
            previous(info);
        }));
    }
    let mut app = App::new(document, config, theme);
    let result = event_loop(&mut terminal, &mut app);
    if app.config.mouse {
        let _ = execute!(std::io::stdout(), DisableMouseCapture);
    }
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| app.draw(frame))?;
        if app.quit {
            return Ok(());
        }
        if event::poll(Duration::from_millis(250))? {
            match event::read()? {
                Event::Key(key) => app.on_key(key),
                Event::Mouse(mouse) => app.on_mouse(mouse),
                _ => {}
            }
        } else {
            app.on_tick();
        }
    }
}
