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
    use super::{base64, osc52};

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
}
