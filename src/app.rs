//! Application state and input handling.

use std::time::{Duration, Instant};

use ratatui::Frame;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;

use crate::clipboard;
use crate::coords;
use crate::document::Document;
use crate::search::Search;
use crate::selection::{self, Point, Selection};
use crate::theme::Theme;
use crate::view::{self, View};
use crate::{config::Config, ui};

/// How long a status message stays on screen.
const MESSAGE_TIMEOUT: Duration = Duration::from_secs(4);

/// What the reader is currently doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Moving the cursor and scrolling.
    Normal,
    /// Selecting text with the keyboard.
    Visual,
    /// Typing a search query.
    Search,
    /// The help popup is open.
    Help,
    /// The outline popup is open.
    Toc,
}

/// The reader.
pub struct App {
    /// Effective configuration.
    pub config: Config,
    /// Active theme.
    pub theme: Theme,
    /// The open document.
    pub document: Document,
    /// Scroll and cursor state.
    pub view: View,
    /// Current mode.
    pub mode: Mode,
    /// Active selection, if any.
    pub selection: Option<Selection>,
    /// Search state.
    pub search: Search,
    /// The query being typed in search mode.
    pub search_input: String,
    /// Whether the next typed character replaces the prefilled query.
    pub search_replace: bool,
    /// Selected entry in the outline popup.
    pub toc_selected: usize,
    /// Transient status message and when it was set.
    pub message: Option<(String, Instant)>,
    /// Area of the last draw.
    pub area: Rect,
    /// Set when the reader should exit.
    pub quit: bool,
    /// Anchor of a mouse drag, when the button is down.
    pub drag_anchor: Option<Point>,
    /// Width the document was rendered at.
    pub content_width: u16,
}

impl App {
    /// Creates an app for an open document.
    #[must_use]
    pub fn new(document: Document, config: Config, theme: Theme) -> Self {
        let content_width = document.width;
        Self {
            config,
            theme,
            document,
            view: View::default(),
            mode: Mode::Normal,
            selection: None,
            search: Search::default(),
            search_input: String::new(),
            search_replace: false,
            toc_selected: 0,
            message: None,
            area: Rect::default(),
            quit: false,
            drag_anchor: None,
            content_width,
        }
    }

    /// The content width for a terminal area.
    #[must_use]
    pub fn content_width_for(area: Rect) -> u16 {
        area.width.saturating_sub(1).max(1)
    }

    /// Called when the terminal size changes.
    pub fn resize(&mut self, area: Rect) {
        self.area = area;
        let width = Self::content_width_for(area);
        if width != self.content_width {
            self.content_width = width;
            self.document.render(width);
        }
        self.view.clamp(&self.document, self.content_height());
        self.view
            .ensure_visible(usize::from(width), self.content_height(), &self.document);
    }

    /// The height of the content area.
    #[must_use]
    pub fn content_height(&self) -> usize {
        let status = if self.config.status_bar { 1 } else { 0 };
        usize::from(self.area.height.saturating_sub(status))
    }

    /// Draws the reader, resizing the layout first when needed.
    pub fn draw(&mut self, frame: &mut Frame) {
        self.resize(frame.area());
        let terminal = frame.area();
        let status_height = u16::from(self.config.status_bar);
        let content = Rect {
            x: terminal.x,
            y: terminal.y,
            width: terminal.width,
            height: terminal.height.saturating_sub(status_height),
        };
        let scene = view::Scene {
            document: &self.document,
            view: &self.view,
            search: &self.search,
            selection: self.selection.as_ref(),
            theme: &self.theme,
            cursor: self.cursor_visible().then_some(self.view.cursor),
        };
        view::draw(frame.buffer_mut(), content, &scene);
        if self.config.status_bar && terminal.height > 0 {
            ui::draw_status(
                frame,
                Rect {
                    x: terminal.x,
                    y: terminal.y + terminal.height - 1,
                    width: terminal.width,
                    height: 1,
                },
                self,
            );
        }
        match self.mode {
            Mode::Help => ui::draw_help(frame, terminal, &self.theme),
            Mode::Toc => ui::draw_toc(frame, terminal, self),
            _ => {}
        }
    }

    /// Handles a key press.
    pub fn on_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            match self.mode {
                Mode::Visual | Mode::Search | Mode::Help | Mode::Toc => {
                    self.cancel();
                }
                Mode::Normal => self.quit = true,
            }
            return;
        }
        match self.mode {
            Mode::Normal => self.handle_normal(key),
            Mode::Visual => self.handle_visual(key),
            Mode::Search => self.handle_search(key),
            Mode::Help => self.handle_popup(key),
            Mode::Toc => self.handle_toc(key),
        }
    }

    fn handle_normal(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Esc => {
                if self.search.is_active() {
                    self.search.clear();
                } else if self.selection.is_some() {
                    self.selection = None;
                }
            }
            KeyCode::Char('j') | KeyCode::Down => self.move_vertical(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_vertical(-1),
            KeyCode::Char('h') | KeyCode::Left => self.move_horizontal(-1),
            KeyCode::Char('l') | KeyCode::Right => self.move_horizontal(1),
            KeyCode::PageDown => self.page(1),
            KeyCode::PageUp => self.page(-1),
            KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.page(1);
            }
            KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.page(-1);
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.half_page(1);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.half_page(-1);
            }
            KeyCode::Char(' ') => self.page(1),
            KeyCode::Char('g') | KeyCode::Home => self.go_top(),
            KeyCode::Char('G') | KeyCode::End => self.go_bottom(),
            KeyCode::Char('w') => self.move_word_forward(),
            KeyCode::Char('b') | KeyCode::Backspace => self.move_word_back(),
            KeyCode::Char('e') => self.move_word_end(),
            KeyCode::Char('0') => self.go_line_start(),
            KeyCode::Char('$') => self.go_line_end(),
            KeyCode::Char('{') => self.move_block(-1),
            KeyCode::Char('}') => self.move_block(1),
            KeyCode::Tab => self.move_heading(1),
            KeyCode::BackTab => self.move_heading(-1),
            KeyCode::Char('v') => self.start_selection(),
            KeyCode::Char('y') => self.copy_default(false),
            KeyCode::Char('Y') => self.copy_default(true),
            KeyCode::Char('t') => self.open_toc(),
            KeyCode::Char('/') => self.open_search(),
            KeyCode::Char('n') => self.search_next(),
            KeyCode::Char('N') => self.search_previous(),
            KeyCode::Char('o') | KeyCode::Enter => self.open_link(),
            KeyCode::Char('r') => self.reload(false),
            KeyCode::Char('R') => self.reload(true),
            KeyCode::Char('?') => self.mode = Mode::Help,
            _ => {}
        }
    }

    fn handle_visual(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Enter => {
                self.copy_selection(false);
                self.selection = None;
                self.mode = Mode::Normal;
            }
            KeyCode::Char('Y') => {
                self.copy_selection(true);
                self.selection = None;
                self.mode = Mode::Normal;
            }
            KeyCode::Esc | KeyCode::Char('v') => self.cancel(),
            KeyCode::Char('o') => self.open_link(),
            KeyCode::Char('n') => self.search_next(),
            KeyCode::Char('N') => self.search_previous(),
            _ => {
                self.handle_normal(key);
                if self.mode == Mode::Normal && self.selection.is_some() {
                    self.sync_selection_head();
                }
            }
        }
    }

    fn handle_search(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                self.mode = Mode::Normal;
                self.search_replace = false;
                if self.search.hits.is_empty() {
                    self.set_message("No matches");
                } else {
                    self.seek_forward();
                }
            }
            KeyCode::Esc => {
                self.search.clear();
                self.search_input.clear();
                self.search_replace = false;
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.search_replace = false;
                self.search_input.pop();
                self.refresh_search();
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search_replace = false;
                self.search_input.clear();
                self.refresh_search();
            }
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.search_replace {
                    self.search_input.clear();
                    self.search_replace = false;
                }
                self.search_input.push(ch);
                self.refresh_search();
            }
            _ => {}
        }
    }

    fn handle_popup(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => self.cancel(),
            _ => {}
        }
    }

    fn handle_toc(&mut self, key: KeyEvent) {
        let count = self.document.headings.len();
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('t') => self.cancel(),
            KeyCode::Char('j') | KeyCode::Down if count > 0 => {
                self.toc_selected = (self.toc_selected + 1).min(count - 1);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.toc_selected = self.toc_selected.saturating_sub(1);
            }
            KeyCode::Char('g') => self.toc_selected = 0,
            KeyCode::Char('G') => self.toc_selected = count.saturating_sub(1),
            KeyCode::Enter => {
                if let Some(heading) = self.document.headings.get(self.toc_selected) {
                    let line = heading.line;
                    self.view.cursor = Point::new(line, 0);
                    self.mode = Mode::Normal;
                    let width = usize::from(self.content_width);
                    let height = self.content_height();
                    self.view.cursor.col = 0;
                    self.view.ensure_visible(width, height, &self.document);
                }
            }
            _ => {}
        }
    }

    fn cancel(&mut self) {
        self.selection = None;
        self.search.clear();
        self.search_input.clear();
        self.mode = Mode::Normal;
    }

    /// Handles a mouse event.
    pub fn on_mouse(&mut self, event: MouseEvent) {
        if !self.config.mouse {
            return;
        }
        match event.kind {
            MouseEventKind::ScrollDown => {
                let height = self.content_height();
                self.view.scroll_by(3, self.document.line_count(), height);
            }
            MouseEventKind::ScrollUp => {
                let height = self.content_height();
                self.view.scroll_by(-3, self.document.line_count(), height);
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if self.mode == Mode::Normal || self.mode == Mode::Visual {
                    let point = self.point_at(event.column, event.row);
                    self.view.cursor = point;
                    self.drag_anchor = Some(point);
                    self.selection = Some(Selection::new(point));
                    self.mode = Mode::Visual;
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let (Some(anchor), true) = (self.drag_anchor, self.mode == Mode::Visual) {
                    let point = self.point_at(event.column, event.row);
                    self.view.cursor = point;
                    self.selection = Some(Selection {
                        anchor,
                        head: point,
                    });
                }
            }
            MouseEventKind::Up(MouseButton::Left) if self.drag_anchor.is_some() => {
                self.drag_anchor = None;
                if self
                    .selection
                    .is_some_and(|selection| !selection.is_empty())
                {
                    if self.config.copy_on_select {
                        self.copy_selection(false);
                    }
                } else {
                    self.selection = None;
                }
                self.mode = Mode::Normal;
            }
            _ => {}
        }
    }

    /// Converts a screen position to a content point.
    fn point_at(&self, column: u16, row: u16) -> Point {
        let area = self.area;
        let status_height = u16::from(self.config.status_bar);
        let content_height = area.height.saturating_sub(status_height);
        let line = self
            .view
            .scroll
            .saturating_add(usize::from(row.saturating_sub(area.y)));
        let line = line.min(
            self.view
                .scroll
                .saturating_add(usize::from(content_height.saturating_sub(1))),
        );
        let column = usize::from(column.saturating_sub(area.x)) + self.view.hscroll;
        let line = line.min(self.document.line_count().saturating_sub(1));
        let column = self
            .document
            .lines()
            .get(line)
            .map_or(0, |line| coords::clamp_column(line, column));
        Point::new(line, column)
    }

    /// Expires messages and reloads the file when it changed.
    pub fn on_tick(&mut self) {
        if let Some((_, when)) = &self.message
            && when.elapsed() > MESSAGE_TIMEOUT
        {
            self.message = None;
        }
        if self.config.live_reload
            && let Ok(true) = self.document.reload_if_changed()
        {
            self.set_message("Reloaded");
            self.view.clamp(&self.document, self.content_height());
            let width = usize::from(self.content_width);
            self.view
                .ensure_visible(width, self.content_height(), &self.document);
        }
    }

    // Movement.

    /// The rendered line under the cursor, if the document is not empty.
    fn current_line(&self) -> Option<&mdk_render::term::Line> {
        self.document.lines().get(self.view.cursor.line)
    }

    fn move_vertical(&mut self, delta: i64) {
        let last = self.document.line_count().saturating_sub(1);
        let target = self.view.cursor.line as i64 + delta;
        self.view.cursor.line = target.clamp(0, last as i64) as usize;
        if let Some(line) = self.current_line() {
            self.view.cursor.col = coords::clamp_column(line, self.view.cursor.col);
        }
        self.after_move();
    }

    fn move_horizontal(&mut self, delta: i64) {
        let Some(line) = self.current_line() else {
            return;
        };
        let current = self.view.cursor.col;
        let next = if delta < 0 {
            coords::prev_char_column(line, current)
        } else {
            coords::next_char_column(line, current)
        };
        if let Some(column) = next {
            self.view.cursor.col = column;
        }
        self.after_move();
    }

    fn move_word_forward(&mut self) {
        let Some(line) = self.current_line() else {
            return;
        };
        let column = coords::next_word_start(line, self.view.cursor.col);
        if column > self.view.cursor.col {
            self.view.cursor.col = column;
        } else if self.view.cursor.line + 1 < self.document.line_count() {
            self.view.cursor.line += 1;
            self.view.cursor.col = 0;
        }
        self.after_move();
    }

    fn move_word_back(&mut self) {
        let Some(line) = self.current_line() else {
            return;
        };
        let column = coords::word_start(line, self.view.cursor.col);
        if column < self.view.cursor.col {
            self.view.cursor.col = column;
        } else if self.view.cursor.line > 0 {
            self.view.cursor.line -= 1;
            if let Some(line) = self.current_line() {
                self.view.cursor.col = coords::word_start(line, usize::MAX);
            }
        }
        self.after_move();
    }

    fn move_word_end(&mut self) {
        let Some(line) = self.current_line() else {
            return;
        };
        let column = coords::word_end(line, self.view.cursor.col);
        self.view.cursor.col = if column > self.view.cursor.col {
            column
        } else {
            coords::next_word_end(line, self.view.cursor.col)
        };
        self.after_move();
    }

    fn go_line_start(&mut self) {
        self.view.cursor.col = 0;
        self.after_move();
    }

    fn go_line_end(&mut self) {
        if let Some(line) = self.current_line() {
            self.view.cursor.col = coords::clamp_column(line, usize::MAX);
        }
        self.after_move();
    }

    fn go_top(&mut self) {
        self.view.cursor = Point::new(0, 0);
        self.view.scroll = 0;
        self.view.hscroll = 0;
        self.after_move();
    }

    fn go_bottom(&mut self) {
        let last = self.document.line_count().saturating_sub(1);
        self.view.cursor = Point::new(last, 0);
        self.view.scroll = last;
        self.after_move();
    }

    fn page(&mut self, direction: i64) {
        let height = self.content_height().saturating_sub(1).max(1);
        self.move_vertical(direction * height as i64);
    }

    fn half_page(&mut self, direction: i64) {
        let half = (self.content_height() / 2).max(1);
        self.move_vertical(direction * half as i64);
    }

    fn move_block(&mut self, direction: i64) {
        let lines = self.document.lines();
        let Some(current) = lines.get(self.view.cursor.line).map(|line| line.block) else {
            return;
        };
        let mut index = self.view.cursor.line as i64 + direction;
        while index >= 0 && (index as usize) < lines.len() {
            let line = &lines[index as usize];
            if line.block.is_some() && line.block != current {
                self.view.cursor = Point::new(index as usize, 0);
                break;
            }
            index += direction;
        }
        self.after_move();
    }

    fn move_heading(&mut self, direction: i64) {
        let current = self.view.cursor.line;
        let target = if direction > 0 {
            self.document
                .headings
                .iter()
                .find(|heading| heading.line > current)
        } else {
            self.document
                .headings
                .iter()
                .rev()
                .find(|heading| heading.line < current)
        };
        if let Some(heading) = target {
            self.view.cursor = Point::new(heading.line, 0);
            self.after_move();
        }
    }

    fn after_move(&mut self) {
        if self.mode == Mode::Visual {
            self.sync_selection_head();
        }
        let width = usize::from(self.content_width);
        let height = self.content_height();
        self.view.ensure_visible(width, height, &self.document);
    }

    fn sync_selection_head(&mut self) {
        if let Some(selection) = &mut self.selection {
            selection.head = self.view.cursor;
        }
    }

    // Selection and copying.

    fn start_selection(&mut self) {
        self.selection = Some(Selection::new(self.view.cursor));
        self.mode = Mode::Visual;
        self.set_message("Selecting: move, then y to copy (Y for source)");
    }

    fn copy_selection(&mut self, raw: bool) {
        let Some(selection) = self.selection else {
            return;
        };
        if selection.is_empty() {
            return;
        }
        let lines: Vec<&mdk_render::term::Line> = self.document.lines().iter().collect();
        let text = if raw {
            selection::raw_extract(&self.document.source, &lines, &selection)
        } else {
            selection::extract(&self.document.source, &lines, &selection)
        };
        self.copy_text(&text);
    }

    /// Copies the active selection, or the block under the cursor when
    /// nothing is selected.
    fn copy_default(&mut self, raw: bool) {
        if self
            .selection
            .is_some_and(|selection| !selection.is_empty())
        {
            self.copy_selection(raw);
            self.selection = None;
        } else {
            self.copy_current(raw);
        }
    }

    /// Copies the block under the cursor, or the current line when there is
    /// no block.
    fn copy_current(&mut self, raw: bool) {
        let line_index = self.view.cursor.line;
        let Some(line) = self.document.lines().get(line_index) else {
            return;
        };
        let Some(block_index) = line.block else {
            let selection = Selection {
                anchor: Point::new(line_index, 0),
                head: Point::new(line_index, usize::MAX),
            };
            let lines: Vec<&mdk_render::term::Line> = self.document.lines().iter().collect();
            let text = selection::extract(&self.document.source, &lines, &selection);
            self.copy_text(&text);
            return;
        };
        if raw {
            let range = self.document.rendered.blocks[block_index].range.clone();
            let text = self.document.source[range].to_string();
            self.copy_text(&text);
            return;
        }
        let Some(selection) = self.block_selection() else {
            return;
        };
        let lines: Vec<&mdk_render::term::Line> = self.document.lines().iter().collect();
        let text = selection::extract(&self.document.source, &lines, &selection);
        self.copy_text(&text);
    }

    /// The selection covering the block under the cursor.
    fn block_selection(&self) -> Option<Selection> {
        let line_index = self.view.cursor.line;
        let block_index = self.document.lines().get(line_index)?.block?;
        let mut first = line_index;
        while first > 0 && self.document.lines()[first - 1].block == Some(block_index) {
            first -= 1;
        }
        let mut last = line_index;
        while last + 1 < self.document.line_count()
            && self.document.lines()[last + 1].block == Some(block_index)
        {
            last += 1;
        }
        Some(Selection {
            anchor: Point::new(first, 0),
            head: Point::new(last, usize::MAX),
        })
    }

    fn copy_text(&mut self, text: &str) {
        if text.trim().is_empty() {
            self.set_message("Nothing to copy");
            return;
        }
        match clipboard::copy(text, self.config.clipboard) {
            Ok(Some(copied)) => {
                let lines = text.lines().count().max(1);
                self.set_message(format!(
                    "Copied {lines} line(s), {} bytes to {}",
                    copied.bytes,
                    copied.target.describe()
                ));
            }
            Ok(None) => self.set_message("Clipboard is disabled"),
            Err(error) => self.set_message(format!("Copy failed: {error}")),
        }
    }

    // Search.

    fn open_search(&mut self) {
        self.mode = Mode::Search;
        self.search_input = self.search.query.clone();
        self.search_replace = true;
        self.refresh_search();
    }

    fn refresh_search(&mut self) {
        let lines = self.document.lines();
        self.search
            .update(&self.search_input, lines, self.config.case_sensitive_search);
    }

    fn seek_forward(&mut self) {
        if let Some(hit) = self.search.seek_forward(self.view.cursor) {
            self.view.cursor = Point::new(hit.line, hit.start);
            self.after_move();
        }
    }

    fn search_next(&mut self) {
        if self.search.hits.is_empty() {
            self.set_message("No matches");
            return;
        }
        let from = Point::new(
            self.view.cursor.line,
            self.view.cursor.col.saturating_add(1),
        );
        if let Some(hit) = self.search.seek_forward(from) {
            self.view.cursor = Point::new(hit.line, hit.start);
            self.after_move();
        }
    }

    fn search_previous(&mut self) {
        if self.search.hits.is_empty() {
            self.set_message("No matches");
            return;
        }
        if let Some(hit) = self.search.seek_backward(self.view.cursor) {
            self.view.cursor = Point::new(hit.line, hit.start);
            self.after_move();
        }
    }

    // Popups and links.

    fn open_toc(&mut self) {
        if self.document.headings.is_empty() {
            self.set_message("No headings");
            return;
        }
        self.toc_selected = self
            .document
            .headings
            .iter()
            .rposition(|heading| heading.line <= self.view.cursor.line)
            .unwrap_or(0);
        self.mode = Mode::Toc;
    }

    fn open_link(&mut self) {
        let Some(link) = self.document.link_at(self.view.cursor) else {
            self.set_message("No link under the cursor");
            return;
        };
        let url = link.url.clone();
        if url.starts_with('#') {
            self.set_message(format!("Anchor {url}"));
            return;
        }
        let opened = std::process::Command::new("xdg-open")
            .arg(&url)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .or_else(|_| {
                std::process::Command::new("open")
                    .arg(&url)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
            });
        match opened {
            Ok(_) => self.set_message(format!("Opening {url}")),
            Err(error) => self.set_message(format!("Cannot open {url}: {error}")),
        }
    }

    fn reload(&mut self, force: bool) {
        let result = if force {
            self.document.reload()
        } else {
            self.document.reload_if_changed()
        };
        match result {
            Ok(true) => {
                self.set_message("Reloaded");
                self.view.clamp(&self.document, self.content_height());
            }
            Ok(false) => self.set_message("No changes"),
            Err(error) => self.set_message(format!("Reload failed: {error}")),
        }
    }

    /// Sets the transient status message.
    pub fn set_message(&mut self, text: impl Into<String>) {
        self.message = Some((text.into(), Instant::now()));
    }

    /// The current status message, if it has not expired.
    #[must_use]
    pub fn message(&self) -> Option<&str> {
        self.message.as_ref().map(|(text, _)| text.as_str())
    }

    /// Whether the cursor should be shown.
    #[must_use]
    pub fn cursor_visible(&self) -> bool {
        matches!(self.mode, Mode::Normal | Mode::Visual)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdk_core::Dialect;

    fn app(markdown: &str) -> App {
        let document = Document::from_source(markdown, Dialect::Gfm, 40);
        App::new(document, Config::default(), Theme::plain())
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn press(app: &mut App, code: KeyCode) {
        app.on_key(key(code));
    }

    #[test]
    fn vertical_movement_clamps_at_document_bounds() {
        let mut app = app("one\n\ntwo\n");
        press(&mut app, KeyCode::Char('j'));
        assert_eq!(app.view.cursor.line, 1);
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Char('j'));
        assert_eq!(app.view.cursor.line, app.document.line_count() - 1);
        press(&mut app, KeyCode::Char('k'));
        press(&mut app, KeyCode::Char('k'));
        press(&mut app, KeyCode::Char('k'));
        assert_eq!(app.view.cursor.line, 0);
    }

    #[test]
    fn block_selection_covers_the_whole_block() {
        let mut app = app("text\n\n```rust\nlet x = 1;\nlet y = 2;\n```\n\nend\n");
        let code_line = app
            .document
            .lines()
            .iter()
            .position(|line| line.kind == mdk_render::term::LineKind::Code)
            .expect("code line");
        app.view.cursor = Point::new(code_line, 0);
        let selection = app.block_selection().expect("selection");
        assert_eq!(selection.start().line, code_line);
        assert_eq!(selection.end().line, code_line + 1);
        let lines: Vec<&mdk_render::term::Line> = app.document.lines().iter().collect();
        assert_eq!(
            selection::extract(&app.document.source, &lines, &selection),
            "let x = 1;\nlet y = 2;"
        );
    }

    #[test]
    fn visual_selection_follows_the_cursor() {
        let mut app = app("alpha beta\n\ngamma delta\n");
        press(&mut app, KeyCode::Char('v'));
        press(&mut app, KeyCode::Char('w'));
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Char('w'));
        let selection = app.selection.expect("selection");
        assert_eq!(selection.anchor, Point::new(0, 0));
        assert_eq!(selection.head, Point::new(2, 6));
    }

    #[test]
    fn search_jumps_to_the_first_hit() {
        let mut app = app("one two\n\nthree two\n");
        press(&mut app, KeyCode::Char('/'));
        for ch in "two".chars() {
            press(&mut app, KeyCode::Char(ch));
        }
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.view.cursor, Point::new(0, 4));
        press(&mut app, KeyCode::Char('n'));
        assert_eq!(app.view.cursor, Point::new(2, 6));
        press(&mut app, KeyCode::Char('N'));
        assert_eq!(app.view.cursor, Point::new(0, 4));
    }

    #[test]
    fn search_input_replaces_the_previous_query() {
        let mut app = app("alpha beta\n");
        press(&mut app, KeyCode::Char('/'));
        for ch in "alpha".chars() {
            press(&mut app, KeyCode::Char(ch));
        }
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.view.cursor.col, 0);
        press(&mut app, KeyCode::Char('/'));
        for ch in "beta".chars() {
            press(&mut app, KeyCode::Char(ch));
        }
        assert_eq!(app.search_input, "beta");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.view.cursor.col, 6);
    }

    #[test]
    fn escape_cancels_search_and_selection() {
        let mut app = app("text\n");
        press(&mut app, KeyCode::Char('/'));
        press(&mut app, KeyCode::Char('t'));
        assert_eq!(app.mode, Mode::Search);
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.search.hits.is_empty());
        press(&mut app, KeyCode::Char('v'));
        assert_eq!(app.mode, Mode::Visual);
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.selection.is_none());
    }

    #[test]
    fn outline_jumps_to_a_heading() {
        let mut app = app("# One\n\ntext\n\n## Two\n\nmore\n");
        press(&mut app, KeyCode::Char('t'));
        assert_eq!(app.mode, Mode::Toc);
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.view.cursor.line, app.document.headings[1].line);
    }

    #[test]
    fn heading_navigation_moves_between_sections() {
        let mut app = app("# One\n\ntext\n\n## Two\n\nmore\n");
        press(&mut app, KeyCode::Tab);
        assert_eq!(app.view.cursor.line, app.document.headings[1].line);
        press(&mut app, KeyCode::BackTab);
        assert_eq!(app.view.cursor.line, app.document.headings[0].line);
    }

    #[test]
    fn block_navigation_skips_to_the_next_block() {
        let mut app = app("first\n\nsecond\n\nthird\n");
        let first = app.document.lines()[0].block;
        press(&mut app, KeyCode::Char('}'));
        let line = app.view.cursor.line;
        assert_ne!(app.document.lines()[line].block, first);
        assert_eq!(line, 2);
    }

    #[test]
    fn mouse_click_maps_to_content_coordinates() {
        let mut app = app("one\n\ntwo three\n");
        app.area = Rect::new(0, 0, 40, 10);
        app.content_width = 39;
        app.view.scroll = 1;
        let point = app.point_at(4, 1);
        assert_eq!(point, Point::new(2, 4));
    }

    #[test]
    fn mouse_drag_selects_and_release_clears_it() {
        use ratatui::crossterm::event::MouseEventKind;
        let mut app = app("one\n\ntwo three\n");
        app.config.clipboard = crate::config::ClipboardChoice::None;
        app.area = Rect::new(0, 0, 20, 5);
        app.resize(app.area);
        let event = |kind| MouseEvent {
            kind,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };
        app.on_mouse(MouseEvent {
            column: 1,
            row: 0,
            ..event(MouseEventKind::Down(MouseButton::Left))
        });
        assert_eq!(app.mode, Mode::Visual);
        app.on_mouse(MouseEvent {
            column: 4,
            row: 2,
            ..event(MouseEventKind::Drag(MouseButton::Left))
        });
        let selection = app.selection.expect("selection");
        assert_eq!(selection.anchor, Point::new(0, 1));
        assert_eq!(selection.head.line, 2);
        app.on_mouse(event(MouseEventKind::Up(MouseButton::Left)));
        assert_eq!(app.mode, Mode::Normal);
        // The selection stays visible after the release so it can be copied
        // again or extended; Escape clears it.
        assert!(app.selection.is_some());
        assert_eq!(app.message(), Some("Clipboard is disabled"));
        app.on_key(key(KeyCode::Esc));
        assert!(app.selection.is_none());
    }

    #[test]
    fn quit_keys_set_the_flag() {
        let mut app = app("text\n");
        press(&mut app, KeyCode::Char('q'));
        assert!(app.quit);
    }
}
