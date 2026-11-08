//! Filesystem reading: directory listings and previews.
//!
//! Everything here is pure-ish (takes a path, returns data) so it can be tested
//! against temporary directories without a terminal.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// How many lines of a text file we show.
pub const PREVIEW_LINES: usize = 100;
/// Hard cap on how many entries a directory preview keeps in memory.
pub const DIR_PREVIEW_LIMIT: usize = 1000;
/// Hard cap on entries read from one directory. Bounds both memory and the
/// per-frame cost of building list rows; a million-file directory would
/// otherwise take ~50s and 700MB.
pub const MAX_ENTRIES: usize = 20_000;
/// Never read more than this much of a file for a preview.
pub const MAX_PREVIEW_BYTES: u64 = 128 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Filesystem type, as reported by the OS.
pub enum Kind {
    /// A directory.
    Directory,
    /// A regular file.
    File,
    /// A symbolic link; [`Entry::is_dir`] says where it points.
    Symlink,
    /// Socket, pipe or device.
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// One row of a directory listing.
pub struct Entry {
    /// File name without its parent.
    pub name: String,
    /// Full path, ready to open.
    pub path: PathBuf,
    /// Filesystem type.
    pub kind: Kind,
    /// Follows symlinks: a link to a directory is `true`.
    pub is_dir: bool,
    /// Size in bytes; 0 for directories.
    pub size: u64,
    /// Unix executable bit; always false on other platforms.
    pub is_exec: bool,
    /// Last modification time, when the platform reports one.
    pub modified: Option<SystemTime>,
}

impl Entry {
    /// Whether the entry itself is a link.
    pub fn is_symlink(&self) -> bool {
        self.kind == Kind::Symlink
    }

    /// Name shown in the UI: directories get a trailing slash.
    pub fn display_name(&self) -> String {
        if self.is_dir {
            format!("{}/", self.name)
        } else {
            self.name.clone()
        }
    }
}

fn make_entry(
    path: PathBuf,
    name: String,
    meta_kind: Kind,
    size: u64,
    is_exec: bool,
    modified: Option<SystemTime>,
) -> Entry {
    // Resolve directories through symlinks so the UI and the sort both agree.
    let is_dir = match meta_kind {
        Kind::Directory => true,
        Kind::Symlink => fs::metadata(&path).map(|m| m.is_dir()).unwrap_or(false),
        _ => false,
    };
    Entry {
        name,
        path,
        kind: meta_kind,
        is_dir,
        size,
        is_exec,
        modified,
    }
}

/// Read a directory. Directories sort first, then case-insensitive by name.
/// Read a directory. Returns the entries and whether the listing was capped.
pub fn read_entries(dir: &Path, show_hidden: bool) -> std::io::Result<(Vec<Entry>, bool)> {
    let mut out = Vec::new();
    let mut truncated = false;
    for item in fs::read_dir(dir)? {
        // A single unreadable entry must not kill the whole listing.
        let Ok(item) = item else { continue };
        let name = item.file_name().to_string_lossy().into_owned();
        if !show_hidden && name.starts_with('.') {
            continue;
        }
        let Ok(file_type) = item.file_type() else {
            continue;
        };
        let kind = if file_type.is_symlink() {
            Kind::Symlink
        } else if file_type.is_dir() {
            Kind::Directory
        } else if file_type.is_file() {
            Kind::File
        } else {
            Kind::Other
        };
        let meta = item.metadata().ok();
        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        let modified = meta.as_ref().and_then(|m| m.modified().ok());
        // Reuse this metadata instead of stat-ing the path a second time:
        // one syscall per entry instead of two (~40% faster listing).
        let is_exec = meta.as_ref().is_some_and(is_executable_meta);
        out.push(make_entry(item.path(), name, kind, size, is_exec, modified));

        // Stop early rather than materialising a million entries.
        if out.len() > MAX_ENTRIES {
            out.pop();
            truncated = true;
            break;
        }
    }
    sort_entries(&mut out);
    Ok((out, truncated))
}

/// Directories first, then case-insensitive by name.
pub fn sort_entries(entries: &mut [Entry]) {
    // `sort_by_cached_key` lowercases once per entry instead of once per
    // comparison; on a 50k-entry directory that is ~1M fewer allocations.
    entries
        .sort_by_cached_key(|entry| (!entry.is_dir, entry.name.to_lowercase(), entry.name.clone()));
}

#[cfg(unix)]
fn is_executable_meta(meta: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.is_file() && meta.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable_meta(_meta: &fs::Metadata) -> bool {
    false
}

/// What the right-hand pane shows for the highlighted entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Preview {
    /// A directory listing.
    Directory {
        /// Up to [`DIR_PREVIEW_LIMIT`] entries.
        entries: Vec<Entry>,
        /// Real number of entries, which may exceed `entries.len()`.
        total: usize,
    },
    /// A text file's first [`PREVIEW_LINES`] lines.
    Text {
        /// The lines to display.
        lines: Vec<String>,
        /// There is more content than we are showing.
        truncated: bool,
        /// Full file size, for the header.
        size: u64,
    },
    /// A file that looks binary.
    Binary {
        /// File size.
        size: u64,
    },
    /// Not a regular file: FIFO, socket, device. Opening these can block
    /// forever (a FIFO with no writer), so we never read them.
    Special {
        /// What kind of special file it is.
        kind: &'static str,
    },
    /// An empty file.
    Empty,
    /// Something went wrong; the string is user-facing.
    Error(String),
}

impl Preview {
    /// Whether there is anything to show.
    pub fn is_empty(&self) -> bool {
        matches!(self, Preview::Empty)
    }
}

/// Build the preview for one entry.
pub fn load_preview(entry: &Entry, show_hidden: bool) -> Preview {
    if entry.is_dir {
        return match read_entries(&entry.path, show_hidden) {
            Ok((all, _)) => {
                let total = all.len();
                let mut entries = all;
                entries.truncate(DIR_PREVIEW_LIMIT);
                Preview::Directory { entries, total }
            }
            Err(err) => Preview::Error(format!("cannot read directory: {err}")),
        };
    }

    match fs::metadata(&entry.path) {
        Ok(meta) if meta.is_dir() => {
            // Symlink pointing at a directory.
            match read_entries(&entry.path, show_hidden) {
                Ok((mut entries, _)) => {
                    let total = entries.len();
                    entries.truncate(DIR_PREVIEW_LIMIT);
                    Preview::Directory { entries, total }
                }
                Err(err) => Preview::Error(format!("cannot read directory: {err}")),
            }
        }
        Ok(meta) if !meta.is_file() => Preview::Special {
            kind: file_kind(&meta),
        },
        Ok(meta) => read_text_preview(&entry.path, meta.len()),
        Err(err) if entry.is_symlink() => Preview::Error(format!("broken symlink: {err}")),
        Err(err) => Preview::Error(format!("cannot stat: {err}")),
    }
}

/// Human label for a non-regular file. Opening these may block indefinitely,
/// so `load_preview` refuses them before any `File::open`.
fn file_kind(meta: &fs::Metadata) -> &'static str {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        let ft = meta.file_type();
        if ft.is_fifo() {
            return "pipe";
        }
        if ft.is_socket() {
            return "socket";
        }
        if ft.is_char_device() {
            return "character device";
        }
        if ft.is_block_device() {
            return "block device";
        }
    }
    let _ = meta;
    "special file"
}

fn read_text_preview(path: &Path, size: u64) -> Preview {
    // `is_file()` keeps out FIFOs and devices, but some pseudo-files
    // (/proc/kmsg, a hung network mount) still block on open or read. A
    // non-blocking descriptor turns that into EAGAIN instead of a frozen UI.
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }

    let file = match options.open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
            return Preview::Special {
                kind: "would block on open",
            };
        }
        Err(err) => return Preview::Error(format!("cannot open: {err}")),
    };

    let mut buf = Vec::new();
    match file.take(MAX_PREVIEW_BYTES).read_to_end(&mut buf) {
        Ok(_) => {}
        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
            return Preview::Special {
                kind: "would block on read",
            };
        }
        Err(err) => return Preview::Error(format!("cannot read: {err}")),
    }
    if size == 0 {
        return Preview::Empty;
    }

    // A NUL byte near the start is the cheapest reliable "this is binary" signal.
    let sniff_len = buf.len().min(8 * 1024);
    if buf[..sniff_len].contains(&0) {
        return Preview::Binary { size };
    }

    // Hitting the byte cap mid-character produces one replacement char; lossy
    // conversion keeps the rest of the file readable instead of failing outright.
    let text = String::from_utf8_lossy(&buf);
    let hit_byte_cap = buf.len() as u64 >= MAX_PREVIEW_BYTES;

    let mut lines = Vec::with_capacity(PREVIEW_LINES);
    let mut saw_more = hit_byte_cap;
    for line in text.split('\n') {
        if lines.len() == PREVIEW_LINES {
            saw_more = true;
            break;
        }
        lines.push(sanitize(line));
    }
    // A trailing newline produces one empty last line; drop it.
    if lines.len() > 1 && lines.last().is_some_and(|l| l.is_empty()) && !saw_more {
        lines.pop();
    }

    Preview::Text {
        lines,
        truncated: saw_more,
        size,
    }
}

/// Tabs become 4 spaces, control characters become spaces. Keeps the grid aligned.
fn sanitize(line: &str) -> String {
    let line = line.strip_suffix('\r').unwrap_or(line);
    let mut out = String::with_capacity(line.len());
    for ch in line.chars() {
        match ch {
            '\t' => out.push_str("    "),
            // C0, DEL and C1 — kept out of the data itself, not just out of the
            // renderer, so previews stay safe if they are ever logged or piped.
            c if c.is_control() => out.push(' '),
            // Bidi overrides and invisible format characters can reorder a line
            // on screen. ZWJ/ZWNJ (U+200C/D) are deliberately NOT stripped:
            // they are what joins emoji and many Indic conjuncts.
            '\u{200b}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}'
            | '\u{feff}' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// `1.2K`, `3.4M`, …
#[must_use]
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes}B")
    } else if value >= 10.0 {
        format!("{value:.0}{}", UNITS[unit])
    } else {
        format!("{value:.1}{}", UNITS[unit])
    }
}

/// Compact relative time: `now`, `5m`, `3h`, `2d`, `4mo`, `1y`.
#[must_use]
pub fn human_age(modified: Option<SystemTime>) -> String {
    human_age_at(modified, SystemTime::now())
}

/// Same as [`human_age`] but relative to a caller-supplied clock, so one frame
/// can format every row against the same instant.
#[must_use]
pub fn human_age_at(modified: Option<SystemTime>, now: SystemTime) -> String {
    let Some(modified) = modified else {
        return String::new();
    };
    let secs = now
        .duration_since(modified)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    const MIN: u64 = 60;
    const HOUR: u64 = 60 * MIN;
    const DAY: u64 = 24 * HOUR;
    const MONTH: u64 = 30 * DAY;
    const YEAR: u64 = 365 * DAY;
    match secs {
        0..60 => "now".to_string(),
        s if s < HOUR => format!("{}m", s / MIN),
        s if s < DAY => format!("{}h", s / HOUR),
        s if s < MONTH => format!("{}d", s / DAY),
        s if s < YEAR => format!("{}mo", s / MONTH),
        s => format!("{}y", s / YEAR),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_sizes() {
        assert_eq!(human_size(0), "0B");
        assert_eq!(human_size(999), "999B");
        assert_eq!(human_size(1024), "1.0K");
        assert_eq!(human_size(1536), "1.5K");
        assert_eq!(human_size(10 * 1024), "10K");
        assert_eq!(human_size(5 * 1024 * 1024), "5.0M");
    }

    #[test]
    fn sanitize_replaces_tabs_and_control_chars() {
        assert_eq!(sanitize("a\tb"), "a    b");
        assert_eq!(sanitize("a\r"), "a");
        assert_eq!(sanitize("a\u{1}b"), "a b");
    }

    #[test]
    fn human_age_boundaries() {
        use std::time::{Duration, SystemTime};
        let ago = |secs: u64| Some(SystemTime::now() - Duration::from_secs(secs));
        assert_eq!(human_age(ago(0)), "now");
        assert_eq!(human_age(ago(59)), "now");
        assert_eq!(human_age(ago(61)), "1m");
        assert_eq!(human_age(ago(3600)), "1h");
        assert_eq!(human_age(ago(86_400)), "1d");
        assert_eq!(human_age(ago(40 * 86_400)), "1mo");
        assert_eq!(human_age(ago(400 * 86_400)), "1y");
        assert_eq!(human_age(None), "");
    }

    #[test]
    fn sanitize_strips_c1_and_zero_width_too() {
        assert_eq!(sanitize("a\u{9b}31m"), "a 31m");
        assert_eq!(sanitize("a\u{200b}b"), "a b");
        // Bidi overrides could reorder the line on screen.
        assert_eq!(sanitize("a\u{202e}b"), "a b");
        // ...but joiners are meaningful and must survive.
        assert_eq!(
            sanitize("\u{1f468}\u{200d}\u{1f469}"),
            "\u{1f468}\u{200d}\u{1f469}"
        );
        assert_eq!(sanitize("\u{915}\u{94d}\u{937}"), "\u{915}\u{94d}\u{937}");
    }

    #[test]
    fn dirs_sort_before_files() {
        let mut v = vec![
            Entry {
                name: "b.txt".into(),
                path: PathBuf::new(),
                kind: Kind::File,
                is_dir: false,
                size: 0,
                is_exec: false,
                modified: None,
            },
            Entry {
                name: "a".into(),
                path: PathBuf::new(),
                kind: Kind::Directory,
                is_dir: true,
                size: 0,
                is_exec: false,
                modified: None,
            },
        ];
        sort_entries(&mut v);
        assert_eq!(v[0].name, "a");
    }
}
