//! Render the UI off-screen and print it as text.
//!
//! Screenshots without a terminal — handy for docs, and for showing an AI
//! exactly what the UI produces:
//!
//! ```text
//! cargo run --bin preview -- ~/Projects 100 24
//! cargo run --bin preview -- ~/Projects 100 24 "jjl/src"
//! ```
//!
//! The optional third argument is a key sequence. Plain characters are typed;
//! these tokens are recognised: `<enter>` `<esc>` `<bs>` `<up>` `<down>` `<tab>`.

use std::io::Write;
use std::path::PathBuf;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use tree_explorer::app::App;
use tree_explorer::ui;
use unicode_width::UnicodeWidthStr;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    // Clamp: TestBackend allocates width*height cells, so `9999 9999` would
    // try to allocate ~200 MB and can OOM the machine.
    let width: u16 = args
        .next()
        .and_then(|w| w.parse().ok())
        .unwrap_or(100)
        .clamp(20, 400);
    let height: u16 = args
        .next()
        .and_then(|h| h.parse().ok())
        .unwrap_or(24)
        .clamp(6, 200);
    let keys = args.next().unwrap_or_default();

    let mut app = App::new(path);
    for code in parse_keys(&keys) {
        app.handle_key(code, KeyModifiers::NONE);
    }

    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| ui::draw(frame, &app)).unwrap();

    // `TestBackend`'s Display impl quotes every row; print the grid ourselves so
    // the output can be pasted into a terminal, a chat, or a doc.
    let buffer = terminal.backend().buffer();
    let area = buffer.area;
    for y in 0..area.height {
        let mut line = String::with_capacity(area.width as usize);
        let mut skip = 0u16;
        for x in 0..area.width {
            if skip > 0 {
                skip -= 1;
                continue;
            }
            let symbol = buffer[(x, y)].symbol();
            line.push_str(symbol);
            // Double-width glyphs own the following cell too.
            let cells = UnicodeWidthStr::width(symbol) as u16;
            if cells > 1 {
                skip = cells - 1;
            }
        }
        if writeln!(std::io::stdout(), "{}", line.trim_end()).is_err() {
            return; // reader closed the pipe (e.g. `| head`)
        }
    }
}

fn parse_keys(input: &str) -> Vec<KeyCode> {
    let mut out = Vec::new();
    let mut rest = input;
    while let Some(ch) = rest.chars().next() {
        if ch == '<' {
            let token_len = rest.find('>').map(|i| i + 1);
            if let Some(len) = token_len {
                let token = &rest[..len];
                let code = match token {
                    "<enter>" => Some(KeyCode::Enter),
                    "<esc>" => Some(KeyCode::Esc),
                    "<bs>" => Some(KeyCode::Backspace),
                    "<up>" => Some(KeyCode::Up),
                    "<down>" => Some(KeyCode::Down),
                    "<tab>" => Some(KeyCode::Tab),
                    _ => None,
                };
                if let Some(code) = code {
                    out.push(code);
                    rest = &rest[len..];
                    continue;
                }
            }
        }
        out.push(KeyCode::Char(ch));
        rest = &rest[ch.len_utf8()..];
    }
    out
}
