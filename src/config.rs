//! Configuration file loading.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

/// Colour scheme selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    /// Pick dark or light from the terminal environment.
    #[default]
    Auto,
    /// Colours that read well on a dark background.
    Dark,
    /// Colours that read well on a light background.
    Light,
    /// No colour, only bold, underline and reverse video.
    None,
}

/// How copied text reaches the system clipboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClipboardChoice {
    /// Use a clipboard command when available, otherwise OSC 52.
    #[default]
    Auto,
    /// Always send the OSC 52 escape sequence.
    Osc52,
    /// Only use a clipboard command (`wl-copy`, `xclip`, `xsel`, `pbcopy`).
    Command,
    /// Do not copy anything.
    None,
}

/// Reader configuration from `config.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Colour scheme.
    pub theme: ThemeChoice,
    /// Copy a mouse selection as soon as the button is released.
    pub copy_on_select: bool,
    /// Enable mouse reporting.
    pub mouse: bool,
    /// Clipboard strategy.
    pub clipboard: ClipboardChoice,
    /// Show the status bar at the bottom of the screen.
    pub status_bar: bool,
    /// Re-render when the file changes on disk.
    pub live_reload: bool,
    /// Case sensitive search; when false, search is smart case.
    pub case_sensitive_search: bool,
    /// Highlight fenced code blocks by language.
    pub syntax_highlighting: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::Auto,
            copy_on_select: true,
            mouse: true,
            clipboard: ClipboardChoice::Auto,
            status_bar: true,
            live_reload: true,
            case_sensitive_search: false,
            syntax_highlighting: true,
        }
    }
}

impl Config {
    /// Loads the configuration, returning warnings for a broken file.
    ///
    /// A missing file is not an error: the defaults apply.
    #[must_use]
    pub fn load(explicit: Option<&Path>) -> (Self, Option<String>) {
        let Some(path) = explicit.map(Path::to_path_buf).or_else(default_path) else {
            return (Self::default(), None);
        };
        match fs::read_to_string(&path) {
            Ok(text) => match toml::from_str::<Self>(&text) {
                Ok(config) => (config, None),
                Err(error) => (
                    Self::default(),
                    Some(format!("{}: {error}", path.display())),
                ),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (Self::default(), None),
            Err(error) => (
                Self::default(),
                Some(format!("{}: {error}", path.display())),
            ),
        }
    }

    /// The path a configuration file is loaded from, if known.
    #[must_use]
    pub fn default_location() -> Option<PathBuf> {
        default_path()
    }

    /// Writes the default configuration to `path`, creating parent folders.
    ///
    /// # Errors
    ///
    /// Returns an error when the file cannot be created or written.
    pub fn write_default(path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        fs::write(path, DEFAULT_TEMPLATE)
            .with_context(|| format!("cannot write {}", path.display()))
    }
}

const DEFAULT_TEMPLATE: &str = "\
# mdp configuration
#
# All keys are optional; delete them to keep the defaults.

# Colour scheme: auto, dark, light or none.
theme = \"auto\"

# Copy a mouse selection as soon as the button is released.
copy_on_select = true

# Enable mouse reporting (wheel, click, drag to select).
mouse = true

# Clipboard strategy: auto, osc52, command or none.
#   auto     use wl-copy/xclip/xsel/pbcopy when installed, else OSC 52
#   osc52    always send the OSC 52 escape sequence (works over SSH)
#   command  only use a clipboard command
clipboard = \"auto\"

# Show a status bar with the file name, position and messages.
status_bar = true

# Re-render when the file changes on disk.
live_reload = true

# Make search case sensitive. When false, a query with an upper-case
# letter is case sensitive and an all-lower-case query is not.
case_sensitive_search = false

# Highlight fenced code blocks using their language tag.
syntax_highlighting = true
";

fn default_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("MDP_CONFIG") {
        return Some(PathBuf::from(path));
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("mdp").join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_used_when_the_file_is_missing() {
        let (config, warning) = Config::load(Some(Path::new("/nonexistent/mdp/config.toml")));
        assert!(warning.is_none());
        assert_eq!(config.theme, ThemeChoice::Auto);
        assert!(config.copy_on_select);
        assert_eq!(config.clipboard, ClipboardChoice::Auto);
    }

    #[test]
    fn parses_a_configuration() {
        let dir = tempfile_dir();
        let path = dir.join("config.toml");
        fs::write(
            &path,
            "theme = \"light\"\nclipboard = \"osc52\"\ncopy_on_select = false\n",
        )
        .expect("write");
        let (config, warning) = Config::load(Some(&path));
        assert!(warning.is_none());
        assert_eq!(config.theme, ThemeChoice::Light);
        assert_eq!(config.clipboard, ClipboardChoice::Osc52);
        assert!(!config.copy_on_select);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reports_unknown_keys() {
        let dir = tempfile_dir();
        let path = dir.join("config.toml");
        fs::write(&path, "colour = \"dark\"\n").expect("write");
        let (config, warning) = Config::load(Some(&path));
        assert!(warning.is_some());
        assert_eq!(config.theme, ThemeChoice::Auto);
        fs::remove_dir_all(&dir).ok();
    }

    fn tempfile_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mdp-config-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }
}
