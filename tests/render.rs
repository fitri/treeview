//! Headless tests: render to a `TestBackend` and assert on the visible text.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use tree_explorer::app::{App, Focus, Mode};
use tree_explorer::theme;
use tree_explorer::ui;
use unicode_width::UnicodeWidthStr;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A throwaway directory tree, removed on drop.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "tree-explorer-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("alpha/nested")).unwrap();
        fs::create_dir_all(root.join("beta")).unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join("alpha/one.txt"), "first line\nsecond line\n").unwrap();
        fs::write(root.join("alpha/nested/deep.txt"), "deep\n").unwrap();
        fs::write(root.join("beta/readme.md"), "# beta\n").unwrap();
        fs::write(root.join("notes.md"), "# notes\nhello\n").unwrap();
        fs::write(root.join("data.bin"), [0u8, 1, 2, 3, 0, 255]).unwrap();
        fs::write(
            root.join("big.txt"),
            (1..=500).map(|i| format!("line {i}\n")).collect::<String>(),
        )
        .unwrap();
        fs::write(root.join(".secret"), "hidden\n").unwrap();
        // Display width != char count: 11 chars, 18+ cells.
        fs::write(
            root.join("\u{65e5}\u{672c}\u{8a9e}\u{30d5}\u{30a1}\u{30a4}\u{30eb}.txt"),
            "cjk\n",
        )
        .unwrap();
        Self { root }
    }

    fn path(&self) -> &Path {
        &self.root
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Render to a plain grid of characters (no quoting, no styles), so tests can
/// assert both on text and on exact widths.
fn render(app: &App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| ui::draw(frame, app)).unwrap();
    let buffer = terminal.backend().buffer();
    let mut out = String::with_capacity((width as usize + 1) * height as usize);
    let width = buffer.area.width;
    for y in 0..buffer.area.height {
        let mut skip = 0u16;
        for x in 0..width {
            if skip > 0 {
                skip -= 1;
                continue;
            }
            let symbol = buffer[(x, y)].symbol();
            out.push_str(symbol);
            // A double-width glyph occupies two cells; the second is a placeholder.
            let cells = UnicodeWidthStr::width(symbol) as u16;
            if cells > 1 {
                skip = cells - 1;
            }
        }
        out.push('\n');
    }
    out
}

fn key(app: &mut App, code: KeyCode) {
    app.handle_key(code, KeyModifiers::NONE);
}

fn typed(app: &mut App, text: &str) {
    for ch in text.chars() {
        key(app, KeyCode::Char(ch));
    }
}

#[test]
fn shows_entries_on_the_left_and_the_selected_directory_on_the_right() {
    let fixture = Fixture::new();
    let app = App::new(fixture.path().to_path_buf());
    let screen = render(&app, 100, 24);

    // left pane: directories first, then files
    assert!(screen.contains("alpha/"), "{screen}");
    assert!(screen.contains("beta/"), "{screen}");
    assert!(screen.contains("notes.md"), "{screen}");

    // right pane previews alpha/ because it is first
    assert!(screen.contains("one.txt"), "{screen}");
    assert!(screen.contains("nested/"), "{screen}");
}

#[test]
fn moving_the_cursor_updates_the_preview() {
    let fixture = Fixture::new();
    let mut app = App::new(fixture.path().to_path_buf());
    key(&mut app, KeyCode::Char('j')); // alpha -> beta
    let screen = render(&app, 100, 24);
    assert!(
        screen.contains("readme.md"),
        "beta preview missing:\n{screen}"
    );
    assert!(
        !screen.contains("one.txt"),
        "still showing alpha:\n{screen}"
    );
}

#[test]
fn enter_promotes_the_highlighted_directory_into_the_left_pane() {
    let fixture = Fixture::new();
    let mut app = App::new(fixture.path().to_path_buf());
    key(&mut app, KeyCode::Enter); // alpha
    assert_eq!(app.left.dir, fixture.path().join("alpha"));
    let screen = render(&app, 100, 24);
    assert!(screen.contains("nested/"), "{screen}");
}

#[test]
fn backspace_goes_up_and_reselects_the_directory_we_came_from() {
    let fixture = Fixture::new();
    let mut app = App::new(fixture.path().to_path_buf());
    key(&mut app, KeyCode::Enter); // into alpha
    key(&mut app, KeyCode::Backspace);
    assert_eq!(app.left.dir, fixture.path().join(""));
    let selected = app.left.selected().unwrap();
    assert_eq!(selected.name, "alpha");
    // the preview follows: we are back on alpha/
    assert!(render(&app, 100, 24).contains("one.txt"));
}

#[test]
fn enter_from_the_right_pane_moves_that_directory_to_the_left() {
    let fixture = Fixture::new();
    let mut app = App::new(fixture.path().to_path_buf());
    key(&mut app, KeyCode::Char('l')); // focus right (alpha/)
    assert_eq!(app.focus, Focus::Right);
    key(&mut app, KeyCode::Enter); // enter nested/
    assert_eq!(app.left.dir, fixture.path().join("alpha/nested"));
    assert_eq!(app.focus, Focus::Left);
    let screen = render(&app, 100, 24);
    assert!(screen.contains("deep.txt"), "{screen}");
}

#[test]
fn search_filters_live_and_esc_restores_everything() {
    let fixture = Fixture::new();
    let mut app = App::new(fixture.path().to_path_buf());
    key(&mut app, KeyCode::Char('/'));
    assert_eq!(app.mode, Mode::Search);

    typed(&mut app, "bet");
    assert_eq!(app.left.matches.len(), 1);
    assert_eq!(app.left.selected().unwrap().name, "beta");
    let screen = render(&app, 100, 24);
    assert!(screen.contains("/bet"), "query not shown:\n{screen}");
    assert!(
        !screen.contains("alpha/"),
        "non-matching entry still listed:\n{screen}"
    );

    // Enter keeps the filter and returns to the pane.
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.left.matches.len(), 1);

    // Esc shows everything again.
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.left.matches.len(), app.left.entries.len());
    assert!(!app.quit, "esc with a filter must not quit");
    assert!(render(&app, 100, 24).contains("alpha/"));
}

#[test]
fn search_filters_the_focused_pane() {
    let fixture = Fixture::new();
    let mut app = App::new(fixture.path().to_path_buf());
    key(&mut app, KeyCode::Char('l')); // focus right (alpha/)
    key(&mut app, KeyCode::Char('/'));
    typed(&mut app, "one");
    let right = app.right.browse().unwrap();
    assert_eq!(right.matches.len(), 1);
    assert_eq!(right.selected().unwrap().name, "one.txt");
    // the left pane is untouched
    assert_eq!(app.left.matches.len(), app.left.entries.len());
}

#[test]
fn text_preview_shows_the_first_hundred_lines() {
    let fixture = Fixture::new();
    let mut app = App::new(fixture.path().to_path_buf());
    // walk to big.txt (dirs sort first, then files alphabetically)
    while app.left.selected().map(|e| e.name.as_str()) != Some("big.txt") {
        key(&mut app, KeyCode::Char('j'));
    }
    let screen = render(&app, 100, 30);
    assert!(screen.contains("line 1"), "{screen}");
    assert!(
        screen.contains("first 100 lines"),
        "truncation not announced:\n{screen}"
    );
    assert!(
        !screen.contains("line 120"),
        "should be truncated:\n{screen}"
    );
}

#[test]
fn binary_files_are_not_printed() {
    let fixture = Fixture::new();
    let mut app = App::new(fixture.path().to_path_buf());
    // data.bin sorts before notes.md
    while app.left.selected().map(|e| e.name.as_str()) != Some("data.bin") {
        key(&mut app, KeyCode::Char('j'));
    }
    let screen = render(&app, 100, 24);
    assert!(screen.contains("binary file"), "{screen}");
}

#[test]
fn dotfiles_are_hidden_until_toggled() {
    let fixture = Fixture::new();
    let mut app = App::new(fixture.path().to_path_buf());
    assert!(!render(&app, 100, 24).contains(".secret"));
    key(&mut app, KeyCode::Char('.'));
    assert!(render(&app, 100, 24).contains(".secret"));
}

#[test]
fn does_not_panic_in_a_tiny_terminal() {
    let fixture = Fixture::new();
    let mut app = App::new(fixture.path().to_path_buf());
    for (w, h) in [(24, 6), (30, 8), (20, 5), (80, 6)] {
        let _ = render(&app, w, h);
        key(&mut app, KeyCode::Enter);
        let _ = render(&app, w, h);
    }
}

#[test]
fn no_row_is_wider_than_the_terminal() {
    let fixture = Fixture::new();
    let mut app = App::new(fixture.path().to_path_buf());
    // 60 is narrow enough to drop the size/age columns.
    for (width, height) in [(60u16, 18u16), (80, 24), (100, 30), (140, 40)] {
        for _ in 0..3 {
            let screen = render(&app, width, height);
            for line in screen.lines() {
                let cells = UnicodeWidthStr::width(line);
                assert!(
                    cells <= width as usize,
                    "row {cells} cells wide at width {width}: {line:?}"
                );
            }
            key(&mut app, KeyCode::Char('j'));
        }
        key(&mut app, KeyCode::Char('i'));
    }
}

#[test]
fn icons_can_be_switched_off_for_plain_terminals() {
    let fixture = Fixture::new();
    let mut app = App::new(fixture.path().to_path_buf());

    let fancy = render(&app, 100, 24);
    assert!(fancy.contains(theme::ICON_DIR), "expected folder glyphs");

    key(&mut app, KeyCode::Char('i'));
    let plain = render(&app, 100, 24);
    assert!(!plain.contains(theme::ICON_DIR), "glyphs should be gone");
    assert!(plain.contains("alpha/"), "names must survive:\n{plain}");
    assert!(plain.contains("notes.md"));
}

#[test]
fn entries_have_distinct_icons_per_type() {
    let fixture = Fixture::new();
    let app = App::new(fixture.path().to_path_buf());
    let screen = render(&app, 100, 24);
    // folder, markdown, and rust-ish icons are not the generic file one
    assert!(screen.contains(theme::ICON_DIR));
    assert!(screen.contains(theme::ICON_MARKDOWN));
}

#[test]
fn wide_characters_do_not_break_the_columns() {
    let fixture = Fixture::new();
    // 4096 bytes -> "4.0K"; a distinctive size we can look for.
    let cjk = fixture.path().join("\u{65e5}\u{672c}\u{8a9e}.txt");
    fs::write(&cjk, vec![b'x'; 4096]).unwrap();

    let app = App::new(fixture.path().to_path_buf());
    let screen = render(&app, 100, 24);

    // The name is 6 chars but 9 cells. Padding computed from char count would
    // push the size/age columns outside the pane, where ratatui clips them.
    assert!(screen.contains("\u{65e5}\u{672c}\u{8a9e}"), "{screen}");
    assert!(
        screen.contains("4.0K"),
        "size column clipped for a wide-char name:\n{screen}"
    );
}

#[test]
fn filter_to_zero_then_back_restores_the_list() {
    let fixture = Fixture::new();
    let mut app = App::new(fixture.path().to_path_buf());
    let before = app.left.matches.len();

    key(&mut app, KeyCode::Char('/'));
    typed(&mut app, "zzz");
    assert_eq!(app.left.matches.len(), 0);
    assert!(app.left.selected().is_none());
    let empty = render(&app, 100, 24);
    assert!(empty.contains("nothing matches"), "{empty}");

    for _ in 0..3 {
        key(&mut app, KeyCode::Backspace);
    }
    assert_eq!(app.left.matches.len(), before);
    assert!(render(&app, 100, 24).contains("notes.md"));
}

#[test]
fn reload_keeps_the_same_entry_selected() {
    let fixture = Fixture::new();
    let mut app = App::new(fixture.path().to_path_buf());
    key(&mut app, KeyCode::Char('j')); // demo layout: alpha -> beta
    assert_eq!(app.left.selected().unwrap().name, "beta");

    // A new file that sorts *before* the selection must not steal the cursor.
    fs::write(fixture.path().join("aaa-new.txt"), "new\n").unwrap();
    key(&mut app, KeyCode::Char('r'));
    assert_eq!(app.left.selected().unwrap().name, "beta");
}

#[cfg(unix)]
#[test]
fn fifos_are_never_opened() {
    let fixture = Fixture::new();
    let fifo = fixture.path().join("a_fifo");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !made {
        return; // mkfifo unavailable; nothing to prove
    }

    let mut app = App::new(fixture.path().to_path_buf());
    while app.left.selected().map(|e| e.name.as_str()) != Some("a_fifo") {
        key(&mut app, KeyCode::Char('j'));
    }
    // If this regresses the test suite hangs on File::open — which is exactly
    // the bug being guarded against.
    let screen = render(&app, 100, 24);
    assert!(screen.contains("not previewed"), "{screen}");
}
