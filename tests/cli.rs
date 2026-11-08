//! The command line: an optional path, and where it lands.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use treeview::cli::{self, Invocation};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A throwaway directory, removed on drop. Unique per case: `cargo test` runs
/// these in parallel.
struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new() -> Self {
        let raw = std::env::temp_dir().join(format!(
            "treeview-cli-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&raw);
        std::fs::create_dir_all(&raw).unwrap();
        Self {
            root: std::fs::canonicalize(&raw).unwrap(),
        }
    }

    fn path(&self) -> &Path {
        &self.root
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn parse(args: &[&str]) -> Result<Invocation, String> {
    cli::parse(args.iter().map(OsString::from))
}

#[test]
fn no_arguments_means_the_current_directory() {
    assert_eq!(parse(&[]), Ok(Invocation::Run(None)));
}

#[test]
fn a_path_is_taken_as_the_start_directory() {
    assert_eq!(
        parse(&["/tmp"]),
        Ok(Invocation::Run(Some(PathBuf::from("/tmp"))))
    );
}

#[test]
fn help_and_version_are_answered_wherever_they_appear() {
    assert_eq!(parse(&["-h"]), Ok(Invocation::Help));
    assert_eq!(parse(&["--help"]), Ok(Invocation::Help));
    assert_eq!(parse(&[".", "--help"]), Ok(Invocation::Help));
    assert_eq!(parse(&["-V"]), Ok(Invocation::Version));
    assert_eq!(parse(&["--version"]), Ok(Invocation::Version));
}

#[test]
fn double_dash_admits_a_path_that_starts_with_a_dash() {
    assert_eq!(
        parse(&["--", "-weird"]),
        Ok(Invocation::Run(Some(PathBuf::from("-weird"))))
    );
}

#[test]
fn unknown_options_and_extra_paths_are_rejected() {
    let err = parse(&["-x"]).unwrap_err();
    assert!(err.contains("unknown option"), "{err}");

    let err = parse(&["/tmp", "/home"]).unwrap_err();
    assert!(err.contains("unexpected argument"), "{err}");
}

#[test]
fn resolve_defaults_to_the_current_directory() {
    let start = cli::resolve(None).unwrap();
    assert_eq!(start.dir, std::env::current_dir().unwrap());
    assert_eq!(start.select, None);
}

#[test]
fn resolve_opens_a_directory_as_given() {
    let scratch = Scratch::new();
    let start = cli::resolve(Some(scratch.path().to_path_buf())).unwrap();
    assert_eq!(start.dir, scratch.path());
    assert_eq!(start.select, None);
}

#[test]
fn resolve_opens_a_files_parent_and_selects_the_file() {
    let scratch = Scratch::new();
    let file = scratch.path().join("notes.md");
    std::fs::write(&file, "hello\n").unwrap();

    let start = cli::resolve(Some(file)).unwrap();
    assert_eq!(start.dir, scratch.path());
    assert_eq!(start.select, Some(OsString::from("notes.md")));
}

#[cfg(unix)]
#[test]
fn resolve_keeps_the_link_name_for_a_symlink_to_a_file() {
    let scratch = Scratch::new();
    std::fs::create_dir_all(scratch.path().join("real")).unwrap();
    std::fs::write(scratch.path().join("real/target.txt"), "hello\n").unwrap();
    let link = scratch.path().join("link.txt");
    std::os::unix::fs::symlink("real/target.txt", &link).unwrap();

    // The cursor goes on `link.txt`, not on the `target.txt` it points at.
    let start = cli::resolve(Some(link)).unwrap();
    assert_eq!(start.dir, scratch.path());
    assert_eq!(start.select, Some(OsString::from("link.txt")));
}

#[cfg(unix)]
#[test]
fn resolve_follows_a_symlink_to_a_directory() {
    let scratch = Scratch::new();
    std::fs::create_dir_all(scratch.path().join("real")).unwrap();
    let link = scratch.path().join("link");
    std::os::unix::fs::symlink("real", &link).unwrap();

    let start = cli::resolve(Some(link)).unwrap();
    assert_eq!(start.dir, scratch.path().join("real"));
    assert_eq!(start.select, None);
}

#[test]
fn resolve_canonicalises_a_relative_path() {
    // `treeview .` must land exactly where `treeview` alone lands.
    let start = cli::resolve(Some(PathBuf::from("."))).unwrap();
    let cwd = std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap();
    assert_eq!(start.dir, cwd);
    assert_eq!(start.select, None);
}

#[test]
fn resolve_reports_a_missing_path_instead_of_opening_an_empty_pane() {
    let missing = std::env::temp_dir().join("treeview-no-such-directory-xyzzy");
    let err = cli::resolve(Some(missing)).unwrap_err();
    assert!(err.contains("cannot open"), "{err}");
}
