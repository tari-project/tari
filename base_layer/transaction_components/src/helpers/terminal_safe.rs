//  Copyright 2026, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

//! Escaping for text that is written to a terminal.

use std::{
    borrow::Cow,
    cmp::Ordering,
    fmt::{self, Display, Formatter, Write},
};

use super::terminal_safe_table::ESCAPE_RANGES;

/// True for characters that must never reach a terminal raw: controls, bidi controls, format, private-use,
/// unassigned and default-ignorable code points, blank-rendering fillers, and backslash.
pub fn needs_escape(c: char) -> bool {
    let fixed = matches!(
        c,
        '\u{0}'..='\u{1f}' |
            '\u{7f}'..='\u{9f}' |
            '\u{61c}' |
            '\u{200b}'..='\u{200f}' |
            '\u{2028}'..='\u{202e}' |
            '\u{2060}'..='\u{2064}' |
            '\u{2066}'..='\u{2069}' |
            '\u{feff}' |
            '\\'
    );
    if fixed {
        return true;
    }
    let cp = u32::from(c);
    ESCAPE_RANGES
        .binary_search_by(|&(start, end)| {
            if end < cp {
                Ordering::Less
            } else if start > cp {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        })
        .is_ok()
}

fn push_escaped(out: &mut String, c: char) {
    match c {
        '\t' => out.push_str("\\t"),
        '\n' => out.push_str("\\n"),
        '\r' => out.push_str("\\r"),
        '\\' => out.push_str("\\\\"),
        c => out.extend(c.escape_unicode()),
    }
}

/// Escapes every character `needs_escape` matches in `s`. Printable text, including non-ASCII,
/// passes through unchanged. Borrowed when nothing needs escaping.
pub fn escape_str_for_terminal(s: &str) -> Cow<'_, str> {
    if !s.chars().any(needs_escape) {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if needs_escape(c) {
            push_escaped(&mut out, c);
        } else {
            out.push(c);
        }
    }
    Cow::Owned(out)
}

/// Decodes `bytes` as UTF-8 chunk by chunk; valid text is escaped per `escape_str_for_terminal`, each invalid byte
/// renders as `\xNN`.
pub fn escape_bytes_for_terminal(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for chunk in bytes.utf8_chunks() {
        out.push_str(&escape_str_for_terminal(chunk.valid()));
        for b in chunk.invalid() {
            // Writing to a String cannot fail
            let _ = write!(out, "\\x{b:02x}");
        }
    }
    out
}

/// Display wrapper: formats `T` to a String, then escapes it.
pub struct TerminalSafe<'a, T: ?Sized>(pub &'a T);

impl<T: Display + ?Sized> Display for TerminalSafe<'_, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let s = self.0.to_string();
        f.write_str(&escape_str_for_terminal(&s))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn escaped_text_passes_printable_unicode_through() {
        assert!(matches!(
            escape_str_for_terminal("Tari 塔 ✓"),
            Cow::Borrowed("Tari 塔 ✓")
        ));
    }

    #[test]
    fn escape_output_format() {
        assert_eq!(escape_str_for_terminal("a\tb\nc\rd\\e"), "a\\tb\\nc\\rd\\\\e");
        assert_eq!(
            escape_str_for_terminal("\u{1b}\u{202e}\u{0}\u{61c}\u{feff}"),
            "\\u{1b}\\u{202e}\\u{0}\\u{61c}\\u{feff}"
        );
        assert_eq!(escape_bytes_for_terminal(&[b'a', 0xff, 0xfe, b'b']), "a\\xff\\xfeb");
        assert_eq!(TerminalSafe("x\u{1b}y").to_string(), "x\\u{1b}y");
    }

    #[test]
    fn invisible_and_unassigned_code_points_are_escaped() {
        let mut points = vec![
            0x00ad, 0x034f, 0x115f, 0x1160, 0x17b4, 0x17b5, 0x180e, 0x2800, 0x3164, 0xffa0, 0xe0001,
        ];
        points.extend(0x206a..=0x206f);
        points.extend(0xfe00..=0xfe0f);
        points.extend(0xfff9..=0xfffb);
        points.extend(0xe0020..=0xe007f);
        // Private use, noncharacter, unassigned
        points.extend([0xe000, 0xf0000, 0xfffe, 0xffff, 0x0378]);
        // Space separators and the Khitan filler
        points.extend([0x00a0, 0x1680, 0x2000, 0x200a, 0x202f, 0x205f, 0x3000, 0x16fe4]);
        // Font-dependent blank glyphs
        points.extend([0x1d159, 0x2d7f, 0x303f]);
        for cp in points {
            let c = char::from_u32(cp).unwrap();
            assert!(needs_escape(c), "U+{cp:04X} not escaped");
            assert_eq!(escape_str_for_terminal(&c.to_string()), format!("\\u{{{cp:x}}}"));
        }
    }

    #[test]
    fn plain_space_is_not_escaped() {
        assert!(!needs_escape(' '));
    }

    #[test]
    fn multi_script_text_passes_through() {
        for s in [
            "Hello, world",
            "塔里 区块链",
            "مرحبا بالعالم",
            "Привет",
            "😀🚀✓",
            "e\u{301}",
        ] {
            assert!(matches!(escape_str_for_terminal(s), Cow::Borrowed(b) if b == s), "{s}");
        }
    }

    #[test]
    fn table_is_sorted_and_versioned() {
        assert_eq!(crate::helpers::terminal_safe_table::UCD_VERSION, "18.0.0");
        let mut prev_end: Option<u32> = None;
        for &(start, end) in ESCAPE_RANGES {
            assert!(start <= end);
            if let Some(prev_end) = prev_end {
                // Sorted and merged: a gap of at least one code point between ranges
                assert!(prev_end < start.saturating_sub(1));
            }
            prev_end = Some(end);
        }
    }
}
