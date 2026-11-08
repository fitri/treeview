//! Rendering. A pure function of `App` — nothing here mutates state.
//!
//! Layout, in one screen:
//!
//! ```text
//!  ~ › Projects › ratatui-test            3 dirs · 4 files   ← chrome bar
//!  ╭┤  tree-explorer  6 ├────────╮╭┤  src  6 ├──────────────╮
//!  │▌   src              4.0K 2h││▌   bin/                  │
//!  │     site/            128B 3d││    app.rs       15K  5m  │
//!  ╰────────────────────────────╯╰──────────────────────────╯
//!  NORMAL   h/l pane  j/k move  …                            ← chrome bar
//! ```

use std::time::SystemTime;

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Fill, List, ListItem, ListState, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Browse, Focus, Mode, RightPane};
use crate::fs::{Entry, Preview, human_age_at, human_size};
/// Per-frame context shared by both panes.
struct View {
    focused: bool,
    icons: bool,
    now: SystemTime,
}

use crate::theme::{
    self, BLUE, CHROME_BG, CYAN, DARK, DIM_TEXT, FAINT, FG, ICON_BLANK, MAGENTA, MUTED, RED,
    SELECT_BG, YELLOW, look,
};

/// Gap between the icon column and the name, and the width of the meta columns.
const META_SIZE: usize = 6;
const META_AGE: usize = 4;
const META_GAP: usize = 2;

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    if area.width < 24 || area.height < 6 {
        frame.render_widget(
            Paragraph::new("terminal too small")
                .style(Style::new().fg(RED))
                .centered(),
            area,
        );
        return;
    }

    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .areas(area);

    render_header(frame, app, header);

    let [left_area, right_area] =
        Layout::horizontal([Constraint::Percentage(45), Constraint::Min(24)]).areas(body);

    let now = SystemTime::now();

    let view = View {
        focused: app.focused(Focus::Left),
        icons: app.icons,
        now,
    };
    render_browse(
        frame,
        &app.left,
        left_area,
        browse_title(&app.left, app.icons),
        &view,
    );

    match &app.right {
        RightPane::Dir(browse) => {
            let view = View {
                focused: app.focused(Focus::Right),
                icons: app.icons,
                now,
            };
            render_browse(
                frame,
                browse,
                right_area,
                browse_title(browse, app.icons),
                &view,
            );
        }
        RightPane::Info {
            preview, scroll, ..
        } => {
            let view = View {
                focused: app.focused(Focus::Right),
                icons: app.icons,
                now,
            };
            render_info(
                frame,
                preview,
                right_area,
                app.right_title(),
                *scroll,
                &view,
            );
        }
    }

    render_footer(frame, app, footer);
}

// ---- chrome ----------------------------------------------------------------

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    frame.render_widget(Fill::new(" ").style(theme::chrome()), area);

    let dirs = app.left.entries.iter().filter(|e| e.is_dir).count();
    let files = app.left.entries.len() - dirs;
    let hidden = if app.show_hidden { " · hidden " } else { "" };
    let counts = if app.icons {
        format!(
            "{} {dirs}   {} {files}{hidden}",
            theme::ICON_DIR,
            theme::ICON_FILE
        )
    } else {
        format!("{dirs} dirs   {files} files{hidden}")
    };
    let counts_width = (UnicodeWidthStr::width(counts.as_str()) as u16).min(area.width / 2);
    let [path_area, counts_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(counts_width)]).areas(area);

    frame.render_widget(
        Paragraph::new(breadcrumb(&app.left.dir, path_area.width)).style(theme::chrome()),
        path_area,
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(counts, Style::new().fg(MUTED))))
            .style(theme::chrome())
            .alignment(Alignment::Right),
        counts_area,
    );
}

/// ` ~ › Projects › ratatui-test ` with each segment in a different accent.
fn breadcrumb(path: &std::path::Path, width: u16) -> Line<'static> {
    let home = std::env::var("HOME").ok();
    let mut parts: Vec<String> = match &home {
        Some(home) if path.starts_with(home) => {
            let rest = path.strip_prefix(home).unwrap_or(path);
            let mut v = vec!["~".to_string()];
            v.extend(
                rest.components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned()),
            );
            v
        }
        _ => path
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect(),
    };
    if parts.is_empty() {
        parts.push("/".to_string());
    }

    // Drop leading segments until the crumb fits, then mark it with an ellipsis.
    let budget = width.saturating_sub(2) as usize;
    let total = |parts: &[String]| -> usize {
        parts
            .iter()
            .map(|part| UnicodeWidthStr::width(part.as_str()))
            .sum::<usize>()
            + parts.len() * 3
    };
    let mut skipped = false;
    while parts.len() > 1 && total(&parts) > budget {
        parts.remove(0);
        skipped = true;
    }

    const ACCENTS: [Color; 4] = [BLUE, CYAN, MAGENTA, theme::GREEN];
    let mut spans = vec![Span::raw(" ")];
    if skipped {
        spans.push(Span::styled("…", Style::new().fg(FAINT)));
        spans.push(Span::styled(" › ", Style::new().fg(FAINT)));
    }
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(" › ", Style::new().fg(FAINT)));
        }
        let color = ACCENTS[index % ACCENTS.len()];
        // The final segment (where you are) is the brightest.
        let style = if index + 1 == parts.len() {
            Style::new().fg(color).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(color)
        };
        spans.push(Span::styled(part.clone(), style));
    }
    Line::from(spans)
}

// ---- panes -----------------------------------------------------------------

fn browse_title(browse: &Browse, icons: bool) -> Line<'static> {
    let name = browse
        .dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| browse.dir.display().to_string());
    let icon = if icons { theme::ICON_DIR } else { ICON_BLANK };
    let count = if browse.is_filtered() {
        // Never let a filtered view look like the whole directory when the
        // listing itself was capped.
        let suffix = if browse.truncated { "+" } else { "" };
        format!("{}/{}{suffix}", browse.matches.len(), browse.entries.len())
    } else if browse.truncated {
        format!("{}+", browse.entries.len())
    } else {
        browse.entries.len().to_string()
    };
    Line::from(vec![
        Span::styled(format!(" {icon} {name} "), Style::new()),
        Span::styled(format!(" {count} "), Style::new().fg(MUTED)),
    ])
}

fn pane_block(title: Line<'static>, focused: bool) -> Block<'static> {
    // The title is a filled chip sitting inside the top border, btop-style.
    let title = title.patch_style(if focused {
        theme::chip(BLUE)
    } else {
        Style::new().bg(CHROME_BG).fg(MUTED)
    });
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(if focused { BLUE } else { FAINT }))
        .title(title)
        .title_alignment(Alignment::Left)
}

fn render_browse(
    frame: &mut Frame,
    browse: &Browse,
    area: Rect,
    title: Line<'static>,
    view: &View,
) {
    let block = pane_block(title, view.focused);

    if let Some(error) = &browse.error {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("{} ", theme::ICON_BINARY), Style::new().fg(RED)),
                Span::styled(error.clone(), Style::new().fg(RED)),
            ]))
            .block(block)
            .centered(),
            area,
        );
        return;
    }

    if browse.matches.is_empty() {
        let message = if browse.is_filtered() {
            "nothing matches"
        } else {
            "empty directory"
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                message,
                Style::new().fg(MUTED).add_modifier(Modifier::ITALIC),
            )))
            .block(block)
            .centered(),
            area,
        );
        return;
    }

    // Two columns of border, then the highlight gutter.
    let content_width = area.width.saturating_sub(2).saturating_sub(2);
    let show_meta = content_width as usize >= 30;

    let dim = if view.focused {
        Modifier::empty()
    } else {
        Modifier::DIM
    };

    // Only build rows that can actually be seen. On a 20k-entry directory
    // materialising every ListItem costs ~50 ms per frame, and the idle
    // redraw then burns CPU for nothing.
    let total = browse.matches.len();
    let inner_height = (area.height.saturating_sub(2) as usize).max(1);
    let height = inner_height.min(total);
    let offset = (browse.pos + 1).saturating_sub(height);

    let items: Vec<ListItem> = browse.matches[offset..offset + height]
        .iter()
        .map(|&index| {
            ListItem::new(entry_line(
                &browse.entries[index],
                content_width,
                view.icons,
                show_meta,
                dim,
                view.now,
            ))
        })
        .collect();

    let cursor_in_window = browse.pos - offset;

    let symbol_color = if view.focused { BLUE } else { FAINT };
    let highlight = if view.focused {
        Style::new().bg(SELECT_BG).add_modifier(Modifier::BOLD)
    } else {
        Style::new().bg(DARK)
    };

    let list = List::new(items)
        .block(block)
        .highlight_symbol(Line::from(Span::styled(
            "▌ ",
            Style::new().fg(symbol_color),
        )))
        .highlight_style(highlight);

    let mut state = ListState::default().with_selected(Some(cursor_in_window));
    frame.render_stateful_widget(list, area, &mut state);
}

fn entry_line(
    entry: &Entry,
    content_width: u16,
    icons: bool,
    show_meta: bool,
    dim: Modifier,
    now: SystemTime,
) -> Line<'static> {
    let look = look(entry);
    let (icon, icon_width) = if icons {
        (look.icon, 1)
    } else {
        (ICON_BLANK, 0)
    };
    let name = entry.display_name();
    let label = format!(" {name}");
    // Display width, not char count: a CJK name is 2 cells per char.
    let label_width = UnicodeWidthStr::width(label.as_str());

    let mut spans = vec![
        Span::styled(icon.to_string(), Style::new().fg(look.color)),
        Span::styled(
            label,
            Style::new()
                .fg(look.color)
                .add_modifier(if entry.is_dir {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                })
                .add_modifier(dim),
        ),
    ];

    if show_meta {
        let size = if entry.is_dir {
            String::new()
        } else {
            human_size(entry.size)
        };
        let age = human_age_at(entry.modified, now);
        let meta_width = META_SIZE + META_GAP + META_AGE;
        let used = icon_width + label_width + meta_width;
        let pad = (content_width as usize).saturating_sub(used).max(1);

        spans.push(Span::raw(" ".repeat(pad)));
        spans.push(Span::styled(
            format!("{size:>META_SIZE$}"),
            Style::new().fg(DIM_TEXT).add_modifier(dim),
        ));
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("{age:>META_AGE$}"),
            Style::new().fg(MUTED).add_modifier(dim),
        ));
    }

    Line::from(spans)
}

fn render_info(
    frame: &mut Frame,
    preview: &Preview,
    area: Rect,
    title: String,
    scroll: u16,
    view: &View,
) {
    let (focused, icons, now) = (view.focused, view.icons, view.now);
    let icon = match preview {
        Preview::Text { .. } => theme::ICON_TEXT,
        Preview::Binary { .. } | Preview::Error(_) => theme::ICON_BINARY,
        Preview::Directory { .. } => theme::ICON_DIR,
        Preview::Special { .. } => theme::ICON_BINARY,
        Preview::Empty => theme::ICON_FILE,
    };
    let icon = if icons { icon } else { ICON_BLANK };
    let block = pane_block(
        Line::from(Span::styled(format!(" {icon} {title} "), Style::new())),
        focused,
    );

    match preview {
        Preview::Text {
            lines,
            truncated,
            size,
        } => {
            let number_color = DIM_TEXT;
            let mut text: Vec<Line> = Vec::with_capacity(lines.len() + 2);
            for (index, line) in lines.iter().enumerate() {
                text.push(Line::from(vec![
                    Span::styled(format!("{:>4} ", index + 1), Style::new().fg(number_color)),
                    Span::styled("│ ", Style::new().fg(FAINT)),
                    Span::styled(line.clone(), Style::new().fg(FG)),
                ]));
            }
            if *truncated {
                text.push(Line::from(""));
                text.push(Line::from(Span::styled(
                    format!(
                        "   … first {} lines of {}   (d / u to scroll)",
                        lines.len(),
                        human_size(*size)
                    ),
                    Style::new().fg(YELLOW).add_modifier(Modifier::ITALIC),
                )));
            }

            frame.render_widget(
                Paragraph::new(Text::from(text))
                    .block(block)
                    .scroll((scroll, 0)),
                area,
            );
        }
        Preview::Special { kind } => frame.render_widget(
            Paragraph::new(Text::from(vec![
                Line::from(vec![
                    Span::styled(format!("{} ", theme::ICON_BINARY), Style::new().fg(YELLOW)),
                    Span::styled(*kind, Style::new().fg(YELLOW)),
                ]),
                Line::from(Span::styled(
                    "not previewed — opening it can block",
                    Style::new().fg(MUTED),
                )),
            ]))
            .block(block)
            .centered(),
            area,
        ),
        Preview::Binary { size } => frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("{} ", theme::ICON_BINARY), Style::new().fg(RED)),
                Span::styled(
                    format!("binary file · {}", human_size(*size)),
                    Style::new().fg(MUTED),
                ),
            ]))
            .block(block)
            .centered(),
            area,
        ),
        Preview::Empty => frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "empty file",
                Style::new().fg(MUTED).add_modifier(Modifier::ITALIC),
            )))
            .block(block)
            .centered(),
            area,
        ),
        Preview::Error(message) => frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("{} ", theme::ICON_BINARY), Style::new().fg(RED)),
                Span::styled(message.clone(), Style::new().fg(RED)),
            ]))
            .block(block)
            .centered(),
            area,
        ),
        Preview::Directory { entries, total } => {
            let content_width = area.width.saturating_sub(2).saturating_sub(2);
            let show_meta = content_width as usize >= 30;
            const SHOWN: usize = 200;
            let mut lines: Vec<Line> = entries
                .iter()
                .take(SHOWN)
                .map(|entry| {
                    entry_line(
                        entry,
                        content_width,
                        icons,
                        show_meta,
                        Modifier::empty(),
                        now,
                    )
                })
                .collect();
            let rendered = lines.len().min(entries.len());
            if *total > rendered {
                lines.push(Line::from(Span::styled(
                    format!("   … {} more", total - rendered),
                    Style::new().fg(MUTED),
                )));
            }
            frame.render_widget(Paragraph::new(Text::from(lines)).block(block), area);
        }
    }
}

// ---- footer ----------------------------------------------------------------

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    frame.render_widget(Fill::new(" ").style(theme::chrome()), area);

    let status = if app.mode == Mode::Search {
        let pane = if app.focus == Focus::Left {
            "left"
        } else {
            "right"
        };
        let count = match app.focus {
            Focus::Left => app.left.matches.len(),
            Focus::Right => app
                .right
                .browse()
                .map(|browse| browse.matches.len())
                .unwrap_or(0),
        };
        format!("{count} match in {pane}  ")
    } else if let Some(status) = &app.status {
        format!("{status}  ")
    } else if let Some(filter) = filtered_query(app) {
        format!("filter: {filter}  ")
    } else {
        String::new()
    };

    let status_width = (UnicodeWidthStr::width(status.as_str()) as u16).min(area.width / 2);
    let [main_area, status_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(status_width)]).areas(area);

    let line = if app.mode == Mode::Search {
        Line::from(vec![
            Span::styled(" SEARCH ", theme::chip(YELLOW)),
            Span::raw(" "),
            Span::styled("/", Style::new().fg(YELLOW)),
            Span::styled(app.query.clone(), Style::new().fg(Color::White)),
            Span::styled("▏", Style::new().fg(YELLOW)),
            Span::raw("  "),
            Span::styled("enter", Style::new().fg(BLUE)),
            Span::styled(" keep ", Style::new().fg(MUTED)),
            Span::styled("esc", Style::new().fg(BLUE)),
            Span::styled(" show all", Style::new().fg(MUTED)),
        ])
    } else {
        let mut spans = vec![Span::styled(
            if app.focus == Focus::Left {
                " LEFT "
            } else {
                " RIGHT "
            },
            theme::chip(BLUE),
        )];
        spans.extend(hints(main_area.width, app.icons));
        Line::from(spans)
    };

    // The real terminal cursor sits after the query while searching.
    if app.mode == Mode::Search {
        let x = main_area.x + 10 + UnicodeWidthStr::width(app.query.as_str()) as u16;
        let x = x.min(area.x + area.width.saturating_sub(1));
        frame.set_cursor_position((x, area.y));
    }

    frame.render_widget(Paragraph::new(line).style(theme::chrome()), main_area);
    if !status.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(status, Style::new().fg(YELLOW))))
                .style(theme::chrome())
                .alignment(Alignment::Right),
            status_area,
        );
    }
}

fn filtered_query(app: &App) -> Option<String> {
    let browse = match app.focus {
        Focus::Left => &app.left,
        Focus::Right => app.right.browse()?,
    };
    if app.mode == Mode::Normal && browse.is_filtered() {
        browse.filter.clone()
    } else {
        None
    }
}

/// Key hints, dropped from the right as the terminal narrows.
fn hints(width: u16, icons: bool) -> Vec<Span<'static>> {
    // `width` is the space actually available to the left of the status text.

    let all: [(&str, &str); 7] = [
        ("h/l", "pane"),
        ("j/k", "move"),
        ("enter", "open"),
        ("bksp", "up"),
        ("/", "find"),
        ("i", if icons { "icons" } else { "ascii" }),
        ("q", "quit"),
    ];
    let keep = match width {
        w if w >= 92 => all.len(),
        w if w >= 76 => 6,
        w if w >= 62 => 4,
        w if w >= 50 => 3,
        _ => 1,
    };
    let mut spans = Vec::new();
    let mut used = 9usize; // the LEFT/RIGHT chip
    for (key, label) in all.iter().take(keep) {
        let cost = 2 + UnicodeWidthStr::width(*key) + 1 + UnicodeWidthStr::width(*label);
        if used + cost > width as usize {
            break;
        }
        used += cost;
        spans.push(Span::raw("  "));
        spans.push(Span::styled(*key, Style::new().fg(BLUE)));
        spans.push(Span::styled(format!(" {label}"), Style::new().fg(MUTED)));
    }
    spans
}
