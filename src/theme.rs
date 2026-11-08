//! Colour palette and file-type looks.
//!
//! Everything visual lives here so the design can be tuned in one place.
//! The palette is Tokyo Night-ish: a dark terminal background with saturated
//! accents, which is what btop / lazygit / yazi all converge on.

use ratatui::style::{Color, Modifier, Style};

use crate::fs::Entry;

// ---- palette ---------------------------------------------------------------

pub const FG: Color = Color::Rgb(0xc0, 0xca, 0xf5);
pub const MUTED: Color = Color::Rgb(0x56, 0x5f, 0x89);
pub const FAINT: Color = Color::Rgb(0x2a, 0x2e, 0x44);
pub const DIM_TEXT: Color = Color::Rgb(0x73, 0x7a, 0xa2);

pub const BLUE: Color = Color::Rgb(0x7a, 0xa2, 0xf7);
pub const CYAN: Color = Color::Rgb(0x7d, 0xcf, 0xff);
pub const GREEN: Color = Color::Rgb(0x9e, 0xce, 0x6a);
pub const ORANGE: Color = Color::Rgb(0xff, 0x9e, 0x64);
pub const YELLOW: Color = Color::Rgb(0xe0, 0xaf, 0x68);
pub const MAGENTA: Color = Color::Rgb(0xbb, 0x9a, 0xf7);
pub const RED: Color = Color::Rgb(0xf7, 0x76, 0x8e);

pub const CHROME_BG: Color = Color::Rgb(0x1f, 0x23, 0x35);
pub const SELECT_BG: Color = Color::Rgb(0x28, 0x34, 0x57);
pub const DARK: Color = Color::Rgb(0x16, 0x16, 0x1e);

// ---- glyphs (Nerd Font v3, verified present in CodeNewRoman Nerd Font) -----

pub const ICON_DIR: &str = "\u{f114}"; // 
pub const ICON_FILE: &str = "\u{f016}"; // 
pub const ICON_LINK: &str = "\u{f0c1}"; // 
pub const ICON_RUST: &str = "\u{e7a8}"; // 
pub const ICON_CODE: &str = "\u{f121}"; // 
pub const ICON_MARKDOWN: &str = "\u{f48a}"; // 
pub const ICON_CONFIG: &str = "\u{e615}"; // 
pub const ICON_JSON: &str = "\u{e60b}"; // 
pub const ICON_SHELL: &str = "\u{f489}"; // 
pub const ICON_IMAGE: &str = "\u{f1c5}"; // 
pub const ICON_ARCHIVE: &str = "\u{f410}"; // 
pub const ICON_LOCK: &str = "\u{f023}"; // 
pub const ICON_GIT: &str = "\u{e702}"; // 
pub const ICON_TEXT: &str = "\u{f0f6}"; // 
pub const ICON_PDF: &str = "\u{f1c1}"; // 
pub const ICON_VIDEO: &str = "\u{f03d}"; // 
pub const ICON_BINARY: &str = "\u{f085}"; // 
pub const ICON_PYTHON: &str = "\u{e73c}"; // 
pub const ICON_JS: &str = "\u{e74e}"; // 

/// Fallback glyph when icons are disabled: two spaces keeps the columns lined up.
pub const ICON_BLANK: &str = "";

#[derive(Debug, Clone, Copy)]
pub struct Look {
    pub icon: &'static str,
    pub color: Color,
}

impl Look {
    const fn new(icon: &'static str, color: Color) -> Self {
        Self { icon, color }
    }
}

/// Pick an icon and a colour from the entry's type and extension.
pub fn look(entry: &Entry) -> Look {
    if entry.is_dir {
        return Look::new(ICON_DIR, BLUE);
    }
    if entry.is_symlink() {
        return Look::new(ICON_LINK, MAGENTA);
    }

    let name = entry.name.as_str();
    let ext = name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "rs" => Look::new(ICON_RUST, ORANGE),
        "py" => Look::new(ICON_PYTHON, YELLOW),
        "js" | "mjs" | "cjs" | "ts" | "tsx" | "jsx" | "go" | "c" | "h" | "cpp" | "hpp" | "java"
        | "rb" | "lua" | "sh" | "bash" | "zsh" | "fish" | "nix" => Look::new(ICON_CODE, GREEN),
        "md" | "markdown" | "txt" | "rst" => Look::new(ICON_MARKDOWN, CYAN),
        "toml" | "yaml" | "yml" | "ini" | "conf" | "cfg" | "env" | "xml" | "csv" => {
            Look::new(ICON_CONFIG, YELLOW)
        }
        "json" | "jsonc" | "lock" => Look::new(ICON_JSON, YELLOW),
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "bmp" | "ico" | "avif" => {
            Look::new(ICON_IMAGE, MAGENTA)
        }
        "zip" | "tar" | "gz" | "xz" | "zst" | "bz2" | "7z" | "rar" => {
            Look::new(ICON_ARCHIVE, ORANGE)
        }
        "pdf" => Look::new(ICON_PDF, RED),
        "mp4" | "mkv" | "webm" | "mov" | "avi" => Look::new(ICON_VIDEO, MAGENTA),
        "so" | "dylib" | "dll" | "bin" | "o" | "a" => Look::new(ICON_BINARY, RED),
        "pem" | "key" | "crt" | "asc" | "gpg" => Look::new(ICON_LOCK, RED),
        _ => {
            // Files with no extension, plus the usual suspects.
            match name {
                "Cargo.toml" | "Cargo.lock" => Look::new(ICON_RUST, ORANGE),
                ".gitignore" | ".gitattributes" | ".gitmodules" => Look::new(ICON_GIT, RED),
                "LICENSE" | "LICENCE" | "COPYING" => Look::new(ICON_TEXT, MUTED),
                "Makefile" | "Dockerfile" | "justfile" => Look::new(ICON_CONFIG, CYAN),
                _ if entry.is_exec => Look::new(ICON_BINARY, RED),
                _ => Look::new(ICON_FILE, FG),
            }
        }
    }
}

// ---- reusable styles -------------------------------------------------------

/// The status/header bars: a solid strip of chrome.
pub fn chrome() -> Style {
    Style::new().bg(CHROME_BG).fg(MUTED)
}

pub fn chip(color: Color) -> Style {
    Style::new().bg(color).fg(DARK).add_modifier(Modifier::BOLD)
}

pub fn key_hint() -> Style {
    Style::new().fg(BLUE)
}
