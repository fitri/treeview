//! Render the UI to a PNG or GIF, without a terminal.
//!
//! This is how `assets/screenshot.png` and `assets/demo.gif` are produced. It
//! draws the real `ui::draw` into a `TestBackend`, then rasterises the buffer
//! cell by cell, so the images show exactly what the app renders — no mockups.
//!
//! ```text
//! cargo run --example shot -- shot.png 100 15 ~/Projects/treeview "jj" \
//!     ~/.local/share/fonts/CodeNewRoman/CodeNewRomanNerdFontMono-Regular.otf
//!
//! # a GIF: every frame is a key sequence, applied from the start each time
//! cargo run --example shot -- gif demo.gif 100 15 ~/Projects/treeview \
//!     "jj|jj<enter>|jj<enter>j" ~/.local/share/fonts/.../Regular.otf
//! ```
//!
//! The font must be a monospace Nerd Font (or any font that carries the icons);
//! the bold sibling is looked up by replacing `-Regular` with `-Bold`, and the
//! example falls back to the regular face when there is none.
//!
//! Key sequences use the same tokens as `examples/preview.rs`: plain characters
//! are pressed, `<enter>` `<esc>` `<bs>` `<up>` `<down>` `<tab>` are recognised.

use std::path::{Path, PathBuf};

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use image::{Rgba, RgbaImage};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::style::{Color, Modifier};
use treeview::app::App;
use treeview::ui;

/// Padding around the terminal grid, in pixels.
const PAD: u32 = 22;
/// Corner radius of the card behind the grid.
const RADIUS: f32 = 10.0;

/// The colours a `Reset` cell falls back to, matching the theme's chrome.
const FALLBACK_BG: [u8; 3] = [0x16, 0x16, 0x1e];
const FALLBACK_FG: [u8; 3] = [0xc0, 0xca, 0xf5];
const PAGE_BG: Rgba<u8> = Rgba([0x0a, 0x0d, 0x13, 0xff]);
const BORDER: [u8; 3] = [0x24, 0x2b, 0x3c];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let gif = args.first().map(String::as_str) == Some("gif");
    let rest = if gif { &args[1..] } else { &args[..] };
    if rest.len() < 5 {
        eprintln!(
            "usage: shot [gif] OUT COLS ROWS DIR \"KEYS[|KEYS…]\" [FONT.otf]\n\
             \n\
             FONT defaults to $TREEVIEW_SHOT_FONT, then a few common Nerd Font \
             locations."
        );
        std::process::exit(2);
    }

    let out = PathBuf::from(&rest[0]);
    let cols: u16 = rest[1].parse().expect("COLS must be a number");
    let rows: u16 = rest[2].parse().expect("ROWS must be a number");
    let dir = PathBuf::from(&rest[3]);
    let keys = &rest[4];
    let font = rest.get(5).map(PathBuf::from).unwrap_or_else(default_font);

    let faces = Faces::load(&font);
    if gif {
        write_gif(&out, cols, rows, &dir, keys, &faces);
    } else {
        let img = draw_frame(cols, rows, &dir, keys, &faces);
        img.save(&out).expect("could not write the PNG");
        println!("wrote {} ({}x{})", out.display(), img.width(), img.height());
    }
}

/// A Nerd Font is not vendored, so look where font managers usually put one.
fn default_font() -> PathBuf {
    if let Ok(path) = std::env::var("TREEVIEW_SHOT_FONT") {
        return PathBuf::from(path);
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let candidates = [
        format!("{home}/.local/share/fonts/CodeNewRoman/CodeNewRomanNerdFontMono-Regular.otf"),
        format!("{home}/.local/share/fonts/JetBrainsMono/JetBrainsMonoNerdFontMono-Regular.ttf"),
        "/usr/share/fonts/truetype/hack/HackNerdFontMono-Regular.ttf".into(),
        "/Library/Fonts/HackNerdFontMono-Regular.ttf".into(),
    ];
    candidates
        .into_iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
        .unwrap_or_else(|| {
            eprintln!("no font found; pass one, or set TREEVIEW_SHOT_FONT");
            std::process::exit(2);
        })
}

struct Faces {
    regular: Vec<u8>,
    bold: Vec<u8>,
    scale: f32,
}

impl Faces {
    fn load(path: &Path) -> Self {
        let regular = std::fs::read(path)
            .unwrap_or_else(|err| panic!("cannot read {}: {err}", path.display()));
        // `…-Regular.otf` → `…-Bold.otf`; falling back keeps one-font setups working.
        let name = path.display().to_string();
        let bold = std::fs::read(name.replace("-Regular", "-Bold")).unwrap_or(regular.clone());
        Self {
            regular,
            bold,
            scale: 16.0,
        }
    }
}

fn parse_keys(input: &str) -> Vec<KeyCode> {
    let mut out = Vec::new();
    let mut rest = input;
    while let Some(ch) = rest.chars().next() {
        if ch == '<'
            && let Some(len) = rest.find('>').map(|i| i + 1)
        {
            let token = &rest[..len];
            let code = match token {
                "<enter>" => Some(KeyCode::Enter),
                "<esc>" => Some(KeyCode::Esc),
                "<bs>" => Some(KeyCode::Backspace),
                "<up>" => Some(KeyCode::Up),
                "<down>" => Some(KeyCode::Down),
                "<tab>" => Some(KeyCode::Tab),
                _ => None,
            };
            if let Some(code) = code {
                out.push(code);
                rest = &rest[len..];
                continue;
            }
        }
        out.push(KeyCode::Char(ch));
        rest = &rest[ch.len_utf8()..];
    }
    out
}

fn draw_frame(cols: u16, rows: u16, dir: &Path, keys: &str, faces: &Faces) -> RgbaImage {
    let mut app = App::new(dir.to_path_buf());
    for code in parse_keys(keys) {
        app.handle_key(code, KeyModifiers::NONE);
    }
    let mut term = Terminal::new(TestBackend::new(cols, rows)).expect("test backend");
    term.draw(|frame| ui::draw(frame, &app)).expect("draw");
    render(term.backend().buffer(), faces)
}

fn write_gif(out: &Path, cols: u16, rows: u16, dir: &Path, keys: &str, faces: &Faces) {
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::{Delay, Frame};

    let file = std::fs::File::create(out).expect("could not create the GIF");
    let mut encoder = GifEncoder::new_with_speed(std::io::BufWriter::new(file), 1);
    encoder.set_repeat(Repeat::Infinite).expect("loop");
    for step in keys.split('|') {
        let img = draw_frame(cols, rows, dir, step, faces);
        // Long enough to read the status line, short enough to sit through.
        let delay = Delay::from_numer_denom_ms(1100, 1);
        encoder
            .encode_frame(Frame::from_parts(img, 0, 0, delay))
            .expect("frame");
    }
    println!(
        "wrote {} ({} frames)",
        out.display(),
        keys.split('|').count()
    );
}

fn rgb(color: Color, fallback: [u8; 3]) -> [u8; 3] {
    match color {
        Color::Rgb(r, g, b) => [r, g, b],
        Color::Black => [0, 0, 0],
        Color::Red => [247, 118, 142],
        Color::Green => [158, 206, 106],
        Color::Yellow => [224, 175, 104],
        Color::Blue => [122, 162, 247],
        Color::Magenta => [187, 154, 247],
        Color::Cyan => [125, 207, 255],
        Color::Gray => [192, 202, 245],
        Color::DarkGray => [86, 95, 137],
        Color::White => [255, 255, 255],
        Color::Indexed(i) => {
            let i = u32::from(i);
            if i < 232 {
                let i = i.saturating_sub(16);
                [
                    (i / 36 * 51) as u8,
                    (i % 36 / 6 * 51) as u8,
                    (i % 6 * 51) as u8,
                ]
            } else {
                let v = (8 + (i - 232) * 10) as u8;
                [v, v, v]
            }
        }
        _ => fallback,
    }
}

fn blend(pixel: &mut Rgba<u8>, fg: [u8; 3], alpha: f32) {
    let bg = pixel.0;
    let mix = |b: u8, f: u8| (f32::from(b) * (1.0 - alpha) + f32::from(f) * alpha).round() as u8;
    pixel.0 = [mix(bg[0], fg[0]), mix(bg[1], fg[1]), mix(bg[2], fg[2]), 255];
}

fn render(buf: &Buffer, faces: &Faces) -> RgbaImage {
    let regular = FontRef::try_from_slice(&faces.regular).expect("regular font");
    let bold = FontRef::try_from_slice(&faces.bold).expect("bold font");
    let scale = PxScale::from(faces.scale);
    let (sreg, sbold) = (regular.as_scaled(scale), bold.as_scaled(scale));

    let cw = sreg.h_advance(sreg.glyph_id('M')).round() as u32;
    let ch = (sreg.ascent() - sreg.descent()).round() as u32;
    let (w, h) = (u32::from(buf.area.width), u32::from(buf.area.height));
    let mut img = RgbaImage::from_pixel(w * cw + PAD * 2, h * ch + PAD * 2, PAGE_BG);

    // A rounded card behind the grid, with a hairline border, so the screenshot
    // reads as an object rather than a cropped rectangle.
    let (pw, ph) = (w * cw, h * ch);
    let (iw, ih) = (img.width() as i32, img.height() as i32);
    for y in 0..ih {
        for x in 0..iw {
            let px = x as f32 + 0.5 - PAD as f32;
            let py = y as f32 + 0.5 - PAD as f32;
            let qx = (px - pw as f32 / 2.0).abs() - (pw as f32 / 2.0 - RADIUS);
            let qy = (py - ph as f32 / 2.0).abs() - (ph as f32 / 2.0 - RADIUS);
            let (ax, ay) = (qx.max(0.0), qy.max(0.0));
            let distance = (ax * ax + ay * ay).sqrt() + qx.max(qy).min(0.0) - RADIUS;
            let fill = (0.5 - distance).clamp(0.0, 1.0);
            if fill <= 0.0 {
                continue;
            }
            let edge = (1.5 - (distance + 0.5).abs()).clamp(0.0, 1.0);
            let base = |i: usize| {
                (f32::from(FALLBACK_BG[i]) * (1.0 - edge) + f32::from(BORDER[i]) * edge) as u8
            };
            let page = PAGE_BG.0;
            let mix = |b: u8, f: u8| (f32::from(b) * (1.0 - fill) + f32::from(f) * fill) as u8;
            img.put_pixel(
                x as u32,
                y as u32,
                Rgba([
                    mix(page[0], base(0)),
                    mix(page[1], base(1)),
                    mix(page[2], base(2)),
                    255,
                ]),
            );
        }
    }

    for y in 0..buf.area.height as usize {
        for x in 0..buf.area.width as usize {
            let cell = &buf[(x as u16, y as u16)];
            let bg = rgb(cell.bg, FALLBACK_BG);
            let fg = rgb(cell.fg, FALLBACK_FG);
            if bg != FALLBACK_BG {
                for py in 0..ch {
                    for px in 0..cw {
                        img.put_pixel(
                            PAD + x as u32 * cw + px,
                            PAD + y as u32 * ch + py,
                            Rgba([bg[0], bg[1], bg[2], 255]),
                        );
                    }
                }
            }
            let symbol = cell.symbol();
            if symbol.trim().is_empty() {
                continue;
            }
            let is_bold = cell.modifier.contains(Modifier::BOLD);
            let (font, scaled) = if is_bold {
                (&bold, &sbold)
            } else {
                (&regular, &sreg)
            };
            let base_x = (PAD + x as u32 * cw) as f32;
            let base_y = (PAD + y as u32 * ch) as f32 + scaled.ascent();
            let mut advance = 0.0f32;
            for c in symbol.chars() {
                let id = scaled.glyph_id(c);
                let glyph = id.with_scale_and_position(
                    faces.scale,
                    ab_glyph::point(base_x + advance, base_y),
                );
                if let Some(outline) = font.outline_glyph(glyph) {
                    let bounds = outline.px_bounds();
                    outline.draw(|gx, gy, coverage| {
                        let px = bounds.min.x as i32 + gx as i32;
                        let py = bounds.min.y as i32 + gy as i32;
                        if px >= 0
                            && py >= 0
                            && (px as u32) < img.width()
                            && (py as u32) < img.height()
                        {
                            blend(img.get_pixel_mut(px as u32, py as u32), fg, coverage);
                        }
                    });
                }
                advance += scaled.h_advance(id);
            }
        }
    }

    img
}
