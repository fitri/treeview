# treeview

A two-pane file explorer for the terminal, built on [ratatui](https://ratatui.rs).
The left pane lists the directory you're in; the right previews whatever is
highlighted — a folder's contents, or the first 100 lines of a file.

![treeview browsing a Rust project: a coloured file list on the left, a Cargo.toml preview with line numbers on the right](https://raw.githubusercontent.com/fitri/treeview/main/assets/screenshot.png)

* **left pane** — entries of the directory you are in (directories first)
* **right pane** — the highlighted entry: its contents, or the first 100 lines of a text file
* binary files and special files (pipes, devices, sockets) show a notice instead of garbage

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/fitri/treeview/main/install.sh | sh
```

Downloads a prebuilt static binary into `~/.local/bin` (override with
`PREFIX=/usr/local/bin`). If there is no release for your platform it builds
from source with cargo.

Other ways:

```sh
cargo install --git https://github.com/fitri/treeview   # from source
cargo build --release                                   # from a checkout
```

## Keys

| Key | Action |
| --- | --- |
| `h` `l` | move focus between the left and right pane |
| `j` `k` | move the cursor inside the focused pane |
| `enter` | open the highlighted directory — it becomes the left pane |
| `bksp` / `-` | go up one level (re-selects the directory you came from) |
| `/` | filter the focused pane, live |
| `enter` in search | keep the filter and go back to the pane |
| `esc` in search | clear the filter and show everything |
| `esc` | clear a filter, or quit when there is none |
| `g` / `G` | jump to the first / last entry |
| `.` | toggle dotfiles |
| `r` | reload both panes |
| `d` / `u`, `PgDn` / `PgUp` | scroll a text preview |
| `i` | toggle Nerd Font icons / plain ASCII |
| `q` | quit |

## Design

Tokyo-Night-ish palette with per-type colour and Nerd Font glyphs:

| | |
| --- | --- |
| directories | blue, bold, outline folder |
| Rust | orange, gear |
| markdown / text | cyan, document |
| config (toml, yaml, json…, ) | yellow |
| code (py, js, go, sh…) | green, `</>` |
| images | magenta, image |
| archives · pdf · binaries | orange / red |
| symlinks | magenta, link |

Icons need a **Nerd Font on the machine you view from** (verified here against
CodeNewRoman Nerd Font Mono). No font? Press `i` for a plain two-column layout —
everything else keeps working. Size and age columns appear only when the pane is
wide enough, so narrow terminals stay readable.

## Structure

```
src/theme.rs   palette, icons, per-extension colours
src/fs.rs      read directories, build previews (no UI types)
src/app.rs     state: two panes, filter, focus, key handling
src/ui.rs      rendering; a pure function of App
src/main.rs    terminal setup + the event loop
examples/preview.rs  render to stdout, no terminal needed
tests/render.rs      headless tests against a temp directory
```

## Commands

```bash
cargo run                      # start in the current directory
cargo run -- ~/Projects        # start somewhere else
cargo test                     # headless UI tests
cargo run --example preview -- . 100 24 "jjl/src"   # screenshot as text
```

The `preview` example accepts a key sequence after the path — plain characters are typed,
`<enter>` `<esc>` `<bs>` `<up>` `<down>` `<tab>` are recognised. It is the
fastest way to see what a change did without a terminal.

## Safety

* **No escape-sequence injection.** File names, the highlighted path, the filter
  query and file contents all pass through ratatui's `Buffer`, which drops
  control characters (C0, DEL, C1) before anything reaches the terminal. A file
  named `pwn<ESC>[2J.txt` renders as inert text. Verified at the byte level.
* **Special files are never opened.** A FIFO with no writer, or `/dev/ptmx`,
  would block `open(2)` forever and freeze the event loop; previews are limited
  to regular files.
* Renders are capped: 128 KiB read per file, 100 preview lines, 1000 directory
  entries evaluated for a preview.
* No network, no shell-outs, no `unsafe`.

## Limitations

* A single directory listing is capped at 20 000 entries (the pane title shows
  a trailing `+`). Without the cap a million-file directory took ~50 s and
  ~700 MB.
* Listings are synchronous: moving the cursor re-reads the highlighted
  directory. On a normal project that is well under a millisecond; on
  `/usr/bin` (1 200 entries) it is about 10 ms per move. A cache would need
  careful invalidation (`r`, `.`, external changes), so there isn't one.
* No syntax highlighting in the preview, no `$EDITOR` integration, no file
  operations — it is a viewer.

## Building

```bash
cargo build --release                                    # 620 KB on x86_64-linux
cargo build --release --target x86_64-unknown-linux-musl # 713 KB, fully static
```

The musl target produces a self-contained binary with no libc dependency, for
Docker scratch images or for copying onto a box you do not control.

## License

MIT — see [LICENSE](LICENSE).
