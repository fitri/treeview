use std::io;
use std::path::PathBuf;
use std::time::Duration;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use tree_explorer::app::App;
use tree_explorer::ui;

fn main() -> io::Result<()> {
    let start = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    // `try_init` instead of `init` so a non-interactive run gets a message
    // instead of a panic + a broken terminal.
    let mut terminal = match ratatui::try_init() {
        Ok(terminal) => terminal,
        Err(err) => {
            eprintln!("tree-explorer needs an interactive terminal: {err}");
            std::process::exit(1);
        }
    };
    let result = run(&mut terminal, App::new(start));
    ratatui::restore();
    result
}

fn run(terminal: &mut DefaultTerminal, mut app: App) -> io::Result<()> {
    while !app.quit {
        terminal.draw(|frame| ui::draw(frame, &app))?;

        // A tick keeps the UI responsive to resizes even with no input; there is
        // no animation yet, so 250 ms is plenty.
        if event::poll(Duration::from_millis(250))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.handle_key(key.code, key.modifiers);
        }
    }
    Ok(())
}
