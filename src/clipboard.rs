//! Putting a path on the user's clipboard.
//!
//! The clipboard write is an [OSC 52] escape sequence rather than a shell-out to
//! `pbcopy` or `wl-copy`. That keeps the binary free of process spawns and
//! platform branches, and — the reason it is worth the trade — it works over
//! SSH, where a shell-out would write to the wrong machine's clipboard.
//!
//! Terminals may ignore the sequence (some ship with clipboard writes disabled),
//! so the caller must not claim the path actually reached the clipboard.
//!
//! Paths are copied verbatim unless they contain a control character, in which
//! case they are POSIX-quoted: a newline in a copied path ends the command when
//! it is pasted into a shell, and an ESC or C1 byte is the same injection class
//! the renderer already guards against — only now the bytes come *from* us.
//!
//! [OSC 52]: https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h3-Operating-System-Commands

/// The 64 symbols of standard Base64, in encoding order.
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// The escape sequence that hands `text` to the terminal's clipboard.
///
/// `\x1b]52;c;…\x07` — OSC 52, target `c` (the clipboard rather than the
/// primary selection), a Base64 payload, and BEL to terminate. BEL is used
/// rather than ST because terminals that accept one accept it, and it is what
/// tmux and neovim emit.
pub fn osc52(text: &str) -> String {
    let mut out = String::with_capacity(text.len().div_ceil(3) * 4 + 8);
    out.push_str("\x1b]52;c;");
    out.push_str(&base64(text.as_bytes()));
    out.push('\x07');
    out
}

/// A path ready to be copied, plus the form to show in the status line.
#[derive(Debug, PartialEq, Eq)]
pub struct Yank {
    /// What goes on the clipboard.
    pub payload: String,
    /// What the status line should say, with control characters visible.
    pub shown: String,
}

/// Decide how to copy `path`.
///
/// A plain path is copied as-is. One containing a control character is quoted
/// for the shell, because the alternative is a paste that runs something else.
#[must_use]
pub fn yank(path: &str) -> Yank {
    let shown = escape_controls(path);
    if path.chars().any(char::is_control) {
        Yank {
            payload: format!("'{}'", path.replace('\'', "'\\''")),
            shown: format!("'{}'", shown.replace('\'', "'\\''")),
        }
    } else {
        Yank {
            payload: path.to_string(),
            shown,
        }
    }
}

/// Write control characters as text, so the status line can be trusted.
///
/// The renderer drops `Cc` before it reaches the terminal, which is right for
/// untrusted names but wrong for a confirmation: it would claim a path that is
/// not the one copied.
fn escape_controls(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for c in path.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_ascii_control() => out.push_str(&format!("\\x{:02x}", c as u32)),
            c if c.is_control() => out.push_str(&format!("\\u{{{:x}}}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Standard Base64 with padding, byte-oriented so non-UTF-8 paths survive.
fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        let triple = (u32::from(chunk[0]) << 16) | (u32::from(b1) << 8) | u32::from(b2);
        out.push(ALPHABET[(triple >> 18) as usize & 0x3f] as char);
        out.push(ALPHABET[(triple >> 12) as usize & 0x3f] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(triple >> 6) as usize & 0x3f] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[triple as usize & 0x3f] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{base64, escape_controls, osc52, yank};

    #[test]
    fn base64_matches_the_reference_vectors() {
        // RFC 4648 §10.
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn base64_encodes_bytes_not_characters() {
        // `é` is two bytes in UTF-8; a char-oriented encoder would emit one.
        assert_eq!(base64("é".as_bytes()), "w6k=");
        assert_eq!(base64("日本語".as_bytes()), "5pel5pys6Kqe");
    }

    #[test]
    fn osc52_wraps_the_payload() {
        assert_eq!(osc52("/tmp"), "\u{1b}]52;c;L3RtcA==\u{7}");
    }

    #[test]
    fn osc52_handles_an_empty_path() {
        assert_eq!(osc52(""), "\u{1b}]52;c;\u{7}");
    }

    #[test]
    fn control_characters_become_visible_escapes() {
        assert_eq!(escape_controls("/tmp/nl_\n_x.txt"), "/tmp/nl_\\n_x.txt");
        assert_eq!(escape_controls("bel_\u{7}_.txt"), "bel_\\x07_.txt");
        assert_eq!(escape_controls("esc_\u{1b}_.txt"), "esc_\\x1b_.txt");
        assert_eq!(escape_controls("c1_\u{9b}31m.txt"), "c1_\\u{9b}31m.txt");
        assert_eq!(escape_controls("/plain/path.rs"), "/plain/path.rs");
    }

    #[test]
    fn a_plain_path_is_copied_untouched() {
        let yanked = yank("/tmp/plain path/file.rs");
        assert_eq!(yanked.payload, "/tmp/plain path/file.rs");
        assert_eq!(yanked.shown, yanked.payload);
    }

    #[test]
    fn a_path_with_a_newline_is_quoted_for_the_shell() {
        let yanked = yank("/tmp/nl_\n_x.txt");
        // The newline stays inside the quotes, so a paste cannot end the command.
        assert_eq!(yanked.payload, "'/tmp/nl_\n_x.txt'");
        // The status line shows where it is instead of hiding it.
        assert_eq!(yanked.shown, "'/tmp/nl_\\n_x.txt'");
    }

    #[test]
    fn an_embedded_single_quote_is_escaped() {
        let yanked = yank("/tmp/it's\nhere.txt");
        assert_eq!(yanked.payload, "'/tmp/it'\\''s\nhere.txt'");
    }
}
