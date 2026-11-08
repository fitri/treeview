//! Application state and key handling.
//!
//! Two panes, both navigable:
//!   * left  — the "current" directory, always a list
//!   * right — the highlighted entry: its contents (a list) or its text (a preview)
//!
//! `h`/`l` move focus, `j`/`k` move inside the focused pane, `enter` opens the
//! highlighted directory (it becomes the left pane), `/` filters the focused
//! pane live.

use std::path::PathBuf;

use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use crate::fs::{self, Entry, Preview};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Left,
    Right,
}

impl Focus {
    pub fn other(self) -> Self {
        match self {
            Focus::Left => Focus::Right,
            Focus::Right => Focus::Left,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Search,
}

/// A directory listing plus a filter and a cursor inside the filtered view.
#[derive(Debug, Clone)]
pub struct Browse {
    pub dir: PathBuf,
    pub entries: Vec<Entry>,
    /// Live filter text; `None` means "show everything".
    pub filter: Option<String>,
    /// Indices into `entries` that match the filter, in display order.
    pub matches: Vec<usize>,
    /// Cursor position *within* `matches`.
    pub pos: usize,
    /// Set when the directory could not be read (permissions, deleted, …).
    pub error: Option<String>,
    /// The listing hit `fs::MAX_ENTRIES`; more entries exist on disk.
    pub truncated: bool,
}

impl Browse {
    #[must_use]
    pub fn new(dir: PathBuf, show_hidden: bool) -> Self {
        let mut browse = Self {
            dir,
            entries: Vec::new(),
            filter: None,
            matches: Vec::new(),
            pos: 0,
            error: None,
            truncated: false,
        };
        browse.reload(show_hidden);
        browse
    }

    pub fn reload(&mut self, show_hidden: bool) {
        // Remember which entry was selected by *name*, not by index: toggling
        // dotfiles or hitting `r` can insert/remove rows above the cursor.
        let anchor = self.selected().map(|entry| entry.name.clone());
        match fs::read_entries(&self.dir, show_hidden) {
            Ok((entries, truncated)) => {
                self.truncated = truncated;
                self.entries = entries;
                self.error = None;
            }
            Err(err) => {
                self.entries.clear();
                self.error = Some(err.to_string());
            }
        }
        self.recompute();
        if let Some(anchor) = anchor
            && let Some(pos) = self
                .matches
                .iter()
                .position(|&index| self.entries[index].name == anchor)
        {
            self.pos = pos;
        }
    }

    /// Recompute `matches` from `filter`, keeping the cursor in range.
    fn recompute(&mut self) {
        self.matches = match &self.filter {
            None => (0..self.entries.len()).collect(),
            Some(query) => {
                let needle = query.to_lowercase();
                self.entries
                    .iter()
                    .enumerate()
                    .filter(|(_, entry)| entry.name.to_lowercase().contains(&needle))
                    .map(|(index, _)| index)
                    .collect()
            }
        };
        self.clamp();
    }

    fn clamp(&mut self) {
        self.pos = self.pos.min(self.matches.len().saturating_sub(1));
    }

    pub fn selected(&self) -> Option<&Entry> {
        self.matches
            .get(self.pos)
            .map(|&index| &self.entries[index])
    }

    pub fn is_filtered(&self) -> bool {
        self.filter.is_some()
    }

    /// Replace the filter and jump back to the top of the new result set.
    pub fn set_filter(&mut self, query: String) {
        self.filter = if query.is_empty() { None } else { Some(query) };
        self.pos = 0;
        self.recompute();
    }

    pub fn clear_filter(&mut self) {
        self.filter = None;
        self.pos = 0;
        self.recompute();
    }

    pub fn move_sel(&mut self, delta: isize) {
        if self.matches.is_empty() {
            return;
        }
        let last = (self.matches.len() - 1) as isize;
        self.pos = (self.pos as isize + delta).clamp(0, last) as usize;
    }

    pub fn to_top(&mut self) {
        self.pos = 0;
    }

    pub fn to_bottom(&mut self) {
        self.pos = self.matches.len().saturating_sub(1);
    }
}

/// What the right-hand side is showing.
#[derive(Debug, Clone)]
pub enum RightPane {
    /// The highlighted entry is a directory: a navigable listing.
    Dir(Browse),
    /// The highlighted entry is a file: a read-only preview.
    Info {
        title: String,
        preview: Preview,
        scroll: u16,
    },
}

impl RightPane {
    pub fn browse(&self) -> Option<&Browse> {
        match self {
            RightPane::Dir(browse) => Some(browse),
            RightPane::Info { .. } => None,
        }
    }

    pub fn browse_mut(&mut self) -> Option<&mut Browse> {
        match self {
            RightPane::Dir(browse) => Some(browse),
            RightPane::Info { .. } => None,
        }
    }

    pub fn title(&self) -> String {
        match self {
            RightPane::Dir(browse) => browse
                .dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| browse.dir.display().to_string()),
            RightPane::Info { title, .. } => title.clone(),
        }
    }
}

pub struct App {
    pub left: Browse,
    pub right: RightPane,
    pub focus: Focus,
    pub mode: Mode,
    /// Search buffer, only meaningful while `mode == Mode::Search`.
    pub query: String,
    pub show_hidden: bool,
    /// Nerd Font icons on/off (`i`). Terminals without the font can fall back
    /// to a plain two-column layout.
    pub icons: bool,
    pub status: Option<String>,
    pub quit: bool,
}

impl App {
    pub fn new(start: PathBuf) -> Self {
        let start = std::fs::canonicalize(&start).unwrap_or(start);
        let left = Browse::new(start, false);
        let mut app = Self {
            left,
            // Placeholder: `refresh_right` below fills it in from the listing
            // we already read, so startup does no extra I/O.
            right: RightPane::Info {
                title: String::new(),
                preview: Preview::Empty,
                scroll: 0,
            },
            focus: Focus::Left,
            mode: Mode::Normal,
            query: String::new(),
            show_hidden: false,
            icons: true,
            status: None,
            quit: false,
        };
        app.refresh_right();
        app
    }

    /// Rebuild the right side from whatever the left pane has highlighted.
    fn refresh_right(&mut self) {
        let Some(entry) = self.left.selected().cloned() else {
            self.right = RightPane::Info {
                title: self
                    .left
                    .dir
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| self.left.dir.display().to_string()),
                preview: Preview::Empty,
                scroll: 0,
            };
            return;
        };

        if entry.is_dir {
            self.right = RightPane::Dir(Browse::new(entry.path.clone(), self.show_hidden));
        } else {
            self.right = RightPane::Info {
                title: entry.display_name(),
                preview: fs::load_preview(&entry, self.show_hidden),
                scroll: 0,
            };
        }
    }

    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }

        match self.mode {
            Mode::Search => self.handle_search_key(code),
            Mode::Normal => self.handle_normal_key(code),
        }
    }

    fn handle_search_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char(c) => {
                self.query.push(c);
                self.apply_query();
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.apply_query();
            }
            // Enter keeps the filter and hands control back to the pane.
            KeyCode::Enter => {
                self.mode = Mode::Normal;
                self.status = None;
            }
            // Esc cancels: show everything again.
            KeyCode::Esc => {
                self.query.clear();
                if let Some(browse) = self.focused_browse_mut() {
                    browse.clear_filter();
                }
                self.after_left_change();
                self.mode = Mode::Normal;
                self.status = None;
            }
            // Arrows still move the cursor while the filter is live.
            KeyCode::Down => self.move_focused(1),
            KeyCode::Up => self.move_focused(-1),
            _ => {}
        }
    }

    fn handle_normal_key(&mut self, code: KeyCode) {
        self.status = None;
        match code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Esc => {
                // Esc clears a filter first; with no filter it quits.
                let had_filter = self.focused_browse_mut().is_some_and(|b| b.is_filtered());
                if had_filter {
                    if let Some(browse) = self.focused_browse_mut() {
                        browse.clear_filter();
                    }
                    self.after_left_change();
                } else {
                    self.quit = true;
                }
            }
            KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Left,
            KeyCode::Char('l') | KeyCode::Right => {
                if self.right.browse().is_some() {
                    self.focus = Focus::Right;
                } else {
                    self.status = Some("this pane is a preview — nothing to navigate".into());
                }
            }
            KeyCode::Char('j') | KeyCode::Down => self.move_focused(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_focused(-1),
            KeyCode::Char('g') => {
                if let Some(browse) = self.focused_browse_mut() {
                    browse.to_top();
                }
            }
            KeyCode::Char('G') => {
                if let Some(browse) = self.focused_browse_mut() {
                    browse.to_bottom();
                }
            }
            KeyCode::Enter => self.open_selected(),
            KeyCode::Backspace | KeyCode::Char('-') => self.go_up(),
            KeyCode::Char('/') => self.start_search(),
            KeyCode::Char('.') => self.toggle_hidden(),
            KeyCode::Char('i') => {
                self.icons = !self.icons;
                self.status = Some(if self.icons {
                    "nerd font icons on".into()
                } else {
                    "plain ascii icons".into()
                });
            }
            KeyCode::Char('r') => self.reload(),
            KeyCode::PageDown | KeyCode::Char('d') => self.scroll_preview(10),
            KeyCode::PageUp | KeyCode::Char('u') => self.scroll_preview(-10),
            _ => {}
        }
    }

    /// The browseable pane that currently has focus, if any (a file preview has
    /// nothing to navigate).
    fn focused_browse_mut(&mut self) -> Option<&mut Browse> {
        match self.focus {
            Focus::Left => Some(&mut self.left),
            Focus::Right => self.right.browse_mut(),
        }
    }

    fn move_focused(&mut self, delta: isize) {
        match self.focus {
            Focus::Left => {
                let before = self.left.pos;
                self.left.move_sel(delta);
                // Pushing `k` at the top of a list should not re-read the
                // highlighted directory from disk.
                if self.left.pos != before {
                    self.after_left_change();
                }
            }
            Focus::Right => {
                if let Some(browse) = self.right.browse_mut() {
                    browse.move_sel(delta);
                }
            }
        }
    }

    /// The left cursor moved: the right pane must follow.
    fn after_left_change(&mut self) {
        if self.focus == Focus::Left {
            self.refresh_right();
        }
    }

    fn start_search(&mut self) {
        // Search whatever pane has focus, starting from the full listing.
        if self.focus == Focus::Right && self.right.browse().is_none() {
            self.status = Some("this pane is a preview — press h and search the list".into());
            return;
        }
        if let Some(browse) = self.focused_browse_mut() {
            browse.clear_filter();
        }
        self.query.clear();
        self.mode = Mode::Search;
        if self.focus == Focus::Left {
            self.refresh_right();
        }
    }

    fn apply_query(&mut self) {
        let query = self.query.clone();
        if let Some(browse) = self.focused_browse_mut() {
            browse.set_filter(query);
        }
        if self.focus == Focus::Left {
            self.refresh_right();
        }
    }

    fn open_selected(&mut self) {
        let target = match self.focus {
            Focus::Left => self.left.selected().cloned(),
            Focus::Right => self.right.browse().and_then(|b| b.selected().cloned()),
        };

        let Some(entry) = target else { return };
        if !entry.is_dir {
            self.status = Some(format!("{} is not a directory", entry.name));
            return;
        }

        // Entering a directory always promotes it into the left pane.
        self.left = Browse::new(entry.path.clone(), self.show_hidden);
        self.focus = Focus::Left;
        self.refresh_right();
    }

    fn go_up(&mut self) {
        let Some(parent) = self.left.dir.parent().map(PathBuf::from) else {
            return;
        };
        let previous = self.left.dir.clone();
        let mut browse = Browse::new(parent, self.show_hidden);
        // Keep the directory we came from under the cursor, so the preview still
        // shows where we were.
        if let Some(name) = previous.file_name() {
            let name = name.to_string_lossy().into_owned();
            if let Some(pos) = browse
                .matches
                .iter()
                .position(|&i| browse.entries[i].name == name)
            {
                browse.pos = pos;
            }
        }
        self.left = browse;
        self.focus = Focus::Left;
        self.refresh_right();
    }

    fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        self.left.reload(self.show_hidden);
        if let Some(browse) = self.right.browse_mut() {
            browse.reload(self.show_hidden);
        }
        self.refresh_right();
        self.status = Some(if self.show_hidden {
            "showing hidden files".into()
        } else {
            "hiding dotfiles".into()
        });
    }

    fn reload(&mut self) {
        self.left.reload(self.show_hidden);
        self.refresh_right();
        self.status = Some("reloaded".into());
    }

    fn scroll_preview(&mut self, delta: i32) {
        if let RightPane::Info { scroll, .. } = &mut self.right {
            let next = *scroll as i32 + delta;
            *scroll = next.max(0) as u16;
        }
    }

    // ---- read-only helpers used by the UI and tests ----

    pub fn right_title(&self) -> String {
        match &self.right {
            RightPane::Dir(_) => self.right.title(),
            RightPane::Info { title, preview, .. } => match preview {
                Preview::Text {
                    size,
                    truncated,
                    lines,
                } => {
                    let mut out = format!("{title}  {}", fs::human_size(*size));
                    // Keep the "there is more" hint visible even at the top of
                    // the file, where the trailing notice is off-screen.
                    if *truncated {
                        out.push_str(&format!("  · first {} lines", lines.len()));
                    }
                    out
                }
                _ => title.clone(),
            },
        }
    }

    pub fn focused(&self, focus: Focus) -> bool {
        self.focus == focus
    }
}
