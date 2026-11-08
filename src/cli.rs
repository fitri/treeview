//! Command-line parsing and start-path resolution.
//!
//! `main` stays thin: everything that can be decided without a terminal is
//! decided here, so it can be tested headlessly. The binary calls [`parse`],
//! then [`resolve`], then hands the [`Start`] to [`crate::app::App`].

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// Text printed by `-h` and `--help`.
pub const HELP: &str = "\
treeview — a two-pane terminal file explorer

USAGE:
    treeview [PATH]

ARGS:
    <PATH>    directory to open, or a file to reveal in its parent
              (default: the current directory)

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

Use `--` before a PATH that starts with a dash.
";

/// What the command line asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invocation {
    /// Print [`HELP`] and exit successfully.
    Help,
    /// Print the version and exit successfully.
    Version,
    /// Open the terminal, optionally at a path.
    Run(Option<PathBuf>),
}

/// Where to open, and which entry to put under the cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Start {
    /// The directory to list.
    pub dir: PathBuf,
    /// Entry to select once the listing is read, when the user named a file.
    pub select: Option<OsString>,
}

/// Turn the arguments — everything after the program name — into an
/// [`Invocation`].
///
/// Errors are `String`s rather than [`std::io::Error`]s because every failure
/// here is the caller's mistake: the binary prints the message and exits with
/// the usage code.
pub fn parse<I>(args: I) -> Result<Invocation, String>
where
    I: IntoIterator<Item = OsString>,
{
    let mut path: Option<PathBuf> = None;
    let mut positional_only = false;
    for arg in args {
        if !positional_only {
            match arg.to_str() {
                // Answered wherever they appear, so `treeview . --help` still
                // helps instead of opening a terminal.
                Some("-h" | "--help") => return Ok(Invocation::Help),
                Some("-V" | "--version") => return Ok(Invocation::Version),
                Some("--") => {
                    positional_only = true;
                    continue;
                }
                // Another flag. `--` is the documented way through for a path
                // that really does begin with a dash.
                Some(flag) if flag.starts_with('-') && flag != "-" => {
                    return Err(format!("unknown option '{flag}'"));
                }
                _ => {}
            }
        }
        if path.replace(PathBuf::from(&arg)).is_some() {
            return Err(format!("unexpected argument '{}'", arg.to_string_lossy()));
        }
    }
    Ok(Invocation::Run(path))
}

/// Turn the optional path into the directory to open and the entry to select.
///
/// * nothing named — the current directory, which is what `treeview` alone did
///   before it took an argument;
/// * a directory — that directory;
/// * a file (or anything else that is not a directory) — its parent, with the
///   file under the cursor, because a terminal cannot preview what is not a
///   directory listing;
/// * a path that does not exist — an error, reported before the terminal is
///   initialised. Silently opening an empty pane is a worse answer.
pub fn resolve(given: Option<PathBuf>) -> Result<Start, String> {
    let Some(given) = given else {
        let dir = std::env::current_dir()
            .map_err(|err| format!("cannot read the current directory: {err}"))?;
        return Ok(Start { dir, select: None });
    };

    // Canonicalise first: the breadcrumb then shows one spelling of the path,
    // and `..` follows the real parent of a symlink rather than its textual
    // parent.
    let path = std::fs::canonicalize(&given)
        .map_err(|err| format!("cannot open '{}': {err}", given.display()))?;
    let meta = std::fs::metadata(&path)
        .map_err(|err| format!("cannot open '{}': {err}", path.display()))?;

    if meta.is_dir() {
        return Ok(Start {
            dir: path,
            select: None,
        });
    }

    // Not a directory. Open the parent and put the cursor on the entry the user
    // typed: for a symlink that is the link itself, not the file it points at,
    // because the link is the row that exists in the parent listing.
    let name = given.file_name().map(OsStr::to_owned);
    let parent = given
        .parent()
        // `Cargo.toml` and `./Cargo.toml` both parent to "", which is not a path
        // at all; the current directory is what was meant.
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let dir = std::fs::canonicalize(parent)
        .map_err(|err| format!("cannot open '{}': {err}", parent.display()))?;
    Ok(Start { dir, select: name })
}
