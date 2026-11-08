//! A two-pane terminal file explorer.
//!
//! * left pane  — entries of the current directory
//! * right pane — the highlighted entry: a directory listing or a text preview
//!
//! `h`/`l` move focus between panes, `j`/`k` move the cursor, `enter` opens the
//! highlighted directory, `/` filters the focused pane live.

#![warn(missing_docs)]

pub mod app;
pub mod fs;
pub mod theme;
pub mod ui;
