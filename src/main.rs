use std::io;
use std::path::PathBuf;
use std::time::Duration;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use treeview::app::App;
use treeview::ui;

const HELP: &str = "\
treeview — a two-pane terminal file explorer

USAGE:
    treeview [PATH]

ARGS:
    <PATH>    directory to start in (default: current directory)

KEYS:
    h / l     focus the left / right pane
    j / k     move the cursor
    enter     open the highlighted directory
    bksp      go up one level
    /         filter the focused pane
    i         toggle Nerd Font icons
    .         toggle dotfiles
    r         reload
    d / u     scroll a text preview
    q         quit

OPTIONS:
    -h, --help       show this help
    -V, --version    show the version
";

fn main() -> io::Result<()> {
    // Handle flags before touching the terminal, so `treeview --version` works
    // from an installer or a shell script with no tty attached.
    let mut start: Option<PathBuf> = None;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return Ok(());
            }
            "-V" | "--version" => {
                println!("treeview {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            other => start = Some(PathBuf::from(other)),
        }
    }
    let start =
        start.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    // `try_init` instead of `init` so a non-interactive run gets a message
    // instead of a panic + a broken terminal.
    let mut terminal = match ratatui::try_init() {
        Ok(terminal) => terminal,
        Err(err) => {
            eprintln!("treeview needs an interactive terminal: {err}");
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
