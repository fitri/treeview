use std::io::{self, Write};
use std::process::ExitCode;
use std::time::Duration;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use treeview::app::App;
use treeview::cli::{self, Invocation, Start};
use treeview::ui;

/// Exit code for a bad command line, matching the convention `getopt` set.
const USAGE_ERROR: u8 = 2;

fn main() -> ExitCode {
    // Everything below the terminal's turn — help, version, bad flags and a
    // missing directory — is answered first, so `treeview --version` still works
    // from an installer with no tty attached.
    let invocation = match cli::parse(std::env::args_os().skip(1)) {
        Ok(invocation) => invocation,
        Err(message) => {
            eprintln!("treeview: {message}\ntry 'treeview --help'");
            return ExitCode::from(USAGE_ERROR);
        }
    };

    let start = match invocation {
        Invocation::Help => {
            print!("{}", cli::HELP);
            return ExitCode::SUCCESS;
        }
        Invocation::Version => {
            println!("treeview {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Invocation::Run(path) => match cli::resolve(path) {
            Ok(start) => start,
            Err(message) => {
                eprintln!("treeview: {message}");
                return ExitCode::FAILURE;
            }
        },
    };

    run(start)
}

fn run(start: Start) -> ExitCode {
    // `try_init` instead of `init` so a non-interactive run gets a message
    // instead of a panic + a broken terminal.
    let mut terminal = match ratatui::try_init() {
        Ok(terminal) => terminal,
        Err(err) => {
            eprintln!("treeview needs an interactive terminal: {err}");
            return ExitCode::FAILURE;
        }
    };

    let mut app = App::new(start.dir);
    if let Some(name) = start.select {
        app.reveal(&name);
    }

    let result = event_loop(&mut terminal, app);
    ratatui::restore();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("treeview: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Emit any clipboard request the app queued this frame.
///
/// OSC 52 goes through the same writer ratatui draws with, so it cannot land in
/// the middle of a half-written frame.
fn copy_pending(terminal: &mut DefaultTerminal, app: &mut App) -> io::Result<()> {
    let Some(text) = app.clipboard.take() else {
        return Ok(());
    };
    terminal
        .backend_mut()
        .write_all(treeview::clipboard::osc52(&text).as_bytes())?;
    terminal.backend_mut().flush()
}

fn event_loop(terminal: &mut DefaultTerminal, mut app: App) -> io::Result<()> {
    while !app.quit {
        terminal.draw(|frame| ui::draw(frame, &app))?;

        // A tick keeps the UI responsive to resizes even with no input; there is
        // no animation yet, so 250 ms is plenty.
        if event::poll(Duration::from_millis(250))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.handle_key(key.code, key.modifiers);
            copy_pending(terminal, &mut app)?;
        }
    }
    Ok(())
}
