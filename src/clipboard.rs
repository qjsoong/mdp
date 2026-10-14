//! Putting copied text on the system clipboard.

use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::config::ClipboardChoice;

/// Where the text ended up, for the status message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// The terminal clipboard, through an OSC 52 escape sequence.
    Osc52,
    /// A clipboard command, by name.
    Command(&'static str),
}

impl Target {
    /// A short description for the status bar.
    #[must_use]
    pub fn describe(self) -> &'static str {
        match self {
            Target::Osc52 => "OSC 52",
            Target::Command(name) => name,
        }
    }
}

/// Result of a copy operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Copied {
    /// Where the text was copied to.
    pub target: Target,
    /// Number of bytes copied.
    pub bytes: usize,
}

/// Copies `text` according to the configured strategy.
///
/// Returns `None` when copying is disabled or the text is empty.
///
/// # Errors
///
/// Returns an error when every available clipboard path failed.
pub fn copy(text: &str, choice: ClipboardChoice) -> Result<Option<Copied>> {
    if text.is_empty() {
        return Ok(None);
    }
    let bytes = text.len();
    let outcome = match choice {
        ClipboardChoice::None => return Ok(None),
        ClipboardChoice::Osc52 => {
            write_stdout(&osc52_sequence(text, tmux()))?;
            Some(Target::Osc52)
        }
        ClipboardChoice::Command => command_copy(text)?.map(Target::Command),
        ClipboardChoice::Auto => match command_copy(text)? {
            Some(name) => Some(Target::Command(name)),
            None => {
                write_stdout(&osc52_sequence(text, tmux()))?;
                Some(Target::Osc52)
            }
        },
    };
    Ok(outcome.map(|target| Copied { target, bytes }))
}

/// The OSC 52 sequence for `text`, wrapped for tmux when needed.
#[must_use]
pub fn osc52_sequence(text: &str, tmux: bool) -> String {
    let payload = STANDARD.encode(text);
    let sequence = format!("\u{1b}]52;c;{payload}\u{7}");
    if tmux {
        let escaped = sequence.replace('\u{1b}', "\u{1b}\u{1b}");
        format!("\u{1b}Ptmux;{escaped}\u{1b}\\")
    } else {
        sequence
    }
}

fn tmux() -> bool {
    std::env::var_os("TMUX").is_some()
}

fn write_stdout(text: &str) -> Result<()> {
    let mut stdout = std::io::stdout();
    stdout
        .write_all(text.as_bytes())
        .context("cannot write to the terminal")?;
    stdout.flush().context("cannot flush the terminal")?;
    Ok(())
}

/// Tries every clipboard command installed on the system.
fn command_copy(text: &str) -> Result<Option<&'static str>> {
    const CANDIDATES: [(&str, &[&str]); 4] = [
        ("wl-copy", &[]),
        ("xclip", &["-selection", "clipboard", "-i"]),
        ("xsel", &["--clipboard", "--input"]),
        ("pbcopy", &[]),
    ];
    for (program, args) in CANDIDATES {
        let Ok(mut child) = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            continue;
        };
        let written = if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(text.as_bytes()).is_ok()
        } else {
            false
        };
        let status = child.wait();
        if written && status.is_ok_and(|status| status.success()) {
            return Ok(Some(program));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osc52_encodes_base64() {
        let sequence = osc52_sequence("hi", false);
        assert_eq!(sequence, "\u{1b}]52;c;aGk=\u{7}");
    }

    #[test]
    fn osc52_wraps_for_tmux() {
        let sequence = osc52_sequence("hi", true);
        assert!(sequence.starts_with("\u{1b}Ptmux;"));
        assert!(sequence.ends_with("\u{1b}\\"));
        assert!(sequence.contains("\u{1b}\u{1b}]52;c;aGk="));
    }

    #[test]
    fn empty_text_is_not_copied() {
        let copied = copy("", ClipboardChoice::Osc52).expect("copy");
        assert!(copied.is_none());
    }

    #[test]
    fn disabled_clipboard_does_nothing() {
        let copied = copy("text", ClipboardChoice::None).expect("copy");
        assert!(copied.is_none());
    }
}
