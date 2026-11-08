# Plan and decision log

Why this project looks the way it does. Written for whoever picks it up next —
human or agent — so settled ground is not re-covered. Each entry records the
decision, the reason, and the evidence where there is measurement behind it.

---

## Scope

**It is** a two-pane viewer: list a directory on the left, preview the
highlighted entry on the right (a directory's contents, or the first 100 lines of
a text file).

**It is not** a file manager. No move, copy, delete, rename, archive or
`$EDITOR` handoff. No syntax highlighting. No configuration file. Each of those
would roughly double the surface area and none of them is why the tool exists.

---

## Naming

| | |
|---|---|
| Chosen | `treeview` for the repo, crate, library and binary |
| Rejected | `tree-view` — taken on crates.io (a tree-structure library) |
| Rejected | `tview` — taken on crates.io (obscure CSV viewer) **and** it is the name of rivo/tview, a 14k-star Go TUI library. Same domain, permanent search shadow. |
| Rejected | `arbor`, `canopy`, `grove`, `bough`, `twig`, `spruce` — all taken on crates.io |

No Linux command is named `treeview`. The names that *do* collide on `PATH` are
`tree` (the classic lister, so avoid) and `tv` (Television, a fuzzy finder).

The vanity URL `treeview.github.io` is unavailable — the GitHub account
`treeview` was registered in 2013 — so Pages lives under the owner instead.

---

## Architecture

**Immediate mode, two live panes.** The left pane is the directory you are in;
the right pane is *itself a navigable listing* when the highlighted entry is a
directory, not a static preview. `enter` on either pane promotes that folder to
the left, which is what makes walking a tree feel like one continuous motion.

**`lib.rs` + thin `main.rs`.** The library exists so the headless tests and the
`preview` example can drive the real `ui::draw`. It is not a published API and
its surface is not promised to be stable.

**`ui::draw` is a pure function of `App`.** All state lives in one struct; the
renderer never mutates. That is what makes `TestBackend` snapshots meaningful and
what keeps the two panes from drifting out of sync.

**Selection is pinned by name across reloads**, not by index — `r` and `.` can
insert or remove rows above the cursor, and an index would silently select a
different file.

---

## Security

The threat model: the user browses a directory they did not create — a
downloaded tarball, a shared mount, a `tmp` dir.

**Control characters never reach the terminal.** Names, paths, the filter query
and file contents all flow through ratatui's `Buffer`, which filters `Cc`
(C0 + DEL + C1) before serialising. Verified at the byte level: a file named
`pwn<ESC>[2J<ESC>]0;HIJACKED<BEL>.txt` renders as inert text. `fs::sanitize`
additionally strips bidi overrides and zero-width format characters at the data
layer, so previews stay safe if ever logged or piped. **ZWJ/ZWNJ are
deliberately preserved** — stripping them mangles joined emoji and Indic
conjuncts.

**Special files are never opened.** A FIFO with no writer, or `/dev/ptmx`,
blocks `open(2)` forever and freezes the event loop with no way to Ctrl-C out.
This was a real, reproducible hang. Previews are restricted to `is_file()`, and
on Unix the open is `O_NONBLOCK` so `/proc` streams and hung mounts degrade to a
notice instead of a freeze. The regression test runs the render on a worker
thread and fails after 2 s rather than hanging CI.

**Bounded work.** 128 KiB read per file, 100 preview lines, 20 000 entries per
listing. A million-file directory previously took ~50 s and ~700 MB.

No network, no shell-outs, no `unsafe`.

---

## Performance

Measured on the development machine; these numbers drove the fixes.

| Change | Before | After |
|---|---|---|
| Render a 20 000-entry directory | 47–71 ms/frame | **1.8 ms/frame** (visible-window rendering) |
| List `/usr/bin` (1 216 entries) | 17.0 ms | **10.6 ms** (one `stat` per entry instead of two) |
| Startup | — | dropped a pointless `read_dir("/")` |

**Windowed rendering was the important one.** Capping entries at 20 000 bounded
memory but not per-frame cost, and with a 250 ms idle redraw that burnt ~200 ms
of CPU per second doing nothing.

**Listings stay synchronous, with no cache.** Moving the cursor re-reads the
highlighted directory. A cache would need invalidation on `r`, `.` and external
changes, and a stale preview the user cannot explain is worse than a 10 ms read.
The limitation is documented rather than papered over.

**Sorting uses `sort_by_cached_key`.** Contested: one review measured the
allocating comparator as faster, another the cached key. Re-benchmarked here at
50 000 mixed names — cached 42 ms vs comparator 141 ms on shuffled input, but
12 ms vs 7.9 ms on already-sorted input. `read_dir` order is effectively
shuffled, so the cached key stays.

---

## Footprint

**The release profile was already optimal** — `opt-level="z"`, fat LTO,
`panic="abort"`, `strip`. Measured: `opt-level=3` costs +97 KB, thin LTO
+94 KB, no strip +2.25 MB.

**Dropping ratatui's default features saves 6 168 bytes**, measured A/B in the
same tree. Initially assumed this was `all-widgets → calendar → time`; that was
wrong — LTO eliminates `time` entirely. The whole saving is `layout-cache`. It
also removes 8 crates from the graph and roughly halves a cold build, which is
the better reason to keep it.

**musl build verified:** 713 KB, fully static, no libc.

---

## Distribution

- **`install.sh`** downloads the matching release tarball into `~/.local/bin`,
  falls back to `cargo install --git`. HTTPS-only via `--proto '=https'`.
  Tested end to end against a mock release server *and* against the real
  published release.
- **`--help` / `--version` are handled before the terminal is initialised**,
  otherwise an installer cannot verify the binary without a tty.
- **`preview` lives in `examples/`**, not `src/bin/` — as a second binary it
  would have been installed as the generic name `preview` on every user's `PATH`.
- **Releases** build on a `v*` tag via `release.yml` (musl + darwin, both arches).

---

## Rejected

| Option | Why not |
|---|---|
| Preview cache | Staleness the user cannot see or invalidate. Documented instead. |
| Async preview loading | Adds a runtime for a stall that is <1 ms in normal use. |
| Syntax highlighting | The biggest remaining polish item, but a large dependency and the preview is deliberately plain. |
| `#![deny(missing_docs)]` on every item | 97 items; the crate is a binary whose lib exists for testability. Types are documented, pedantic lints are not chased. |
| Windows release binaries | It compiles and CI tests it, but it has only been exercised on Unix — claiming a Windows binary would be overclaiming. |

---

## Execution log

| commit | |
|---|---|
| `4d8dc60` | the app: two panes, filter, previews, theme |
| `c883f24` | perf: single stat, entry cap, no startup I/O |
| `bade5ab` | `/proc` hang, joiner handling, windowed rendering |
| `31a1aab` | docs on the public API, manifest metadata, preview clamp |
| `76f3aa5` | musl build note |
| `de6b8d2` | rename to `treeview`, `install.sh`, Pages site |
| `83ffd55` | real screenshot in the README |
| `04dda81` | README restructure (tagline, badges, features) |

---

## Open questions

1. **GitHub Pages URL.** `fitri/fitri.github.io` carries `CNAME = fitri.me`, but
   `fitri.me` resolves to Cloudflare, not GitHub Pages — so every page under
   `fitri.github.io/*` 301s to a host that serves the personal site for every
   path. The site *is* deployed; it is unreachable. Options: drop that CNAME,
   point `fitri.me` DNS at Pages, or use `treeview.fitri.me`.
2. **crates.io.** `treeview` is available and the manifest is ready
   (`cargo package` builds from the tarball). A crates.io badge should be added
   on the day it is published, not before.
3. **Windows** is built and tested in CI but never run interactively.
