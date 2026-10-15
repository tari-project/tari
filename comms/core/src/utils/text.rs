// Copyright 2026, The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

//! Making peer-supplied text safe to log and display.

/// True for characters in the Unicode general category `Cf` (format), e.g. bidi overrides, zero-width spaces and
/// joiners and the byte order mark.
pub fn is_format_char(c: char) -> bool {
    matches!(
        u32::from(c),
        0x00AD |
            0x0600..=0x0605 |
            0x061C |
            0x06DD |
            0x070F |
            0x0890..=0x0891 |
            0x08E2 |
            0x180E |
            0x200B..=0x200F |
            0x202A..=0x202E |
            0x2060..=0x2064 |
            0x2066..=0x206F |
            0xFEFF |
            0xFFF9..=0xFFFB |
            0x110BD |
            0x110CD |
            0x13430..=0x1343F |
            0x1BCA0..=0x1BCA3 |
            0x1D173..=0x1D17A |
            0xE0001 |
            0xE0020..=0xE007F
    )
}

/// True for format characters (see [`is_format_char`]) and the Unicode line and paragraph separators (U+2028,
/// U+2029), which are not control characters but still break a line.
pub fn is_invisible_or_line_breaking(c: char) -> bool {
    is_format_char(c) || c == '\u{2028}' || c == '\u{2029}'
}

/// Returns `text` without control, format or line/paragraph separator characters, cut to at most `max_bytes` bytes on
/// a character boundary.
pub fn sanitize_for_display(text: &str, max_bytes: usize) -> String {
    let mut sanitized = String::with_capacity(text.len().min(max_bytes));
    for c in text.chars() {
        if c.is_control() || is_invisible_or_line_breaking(c) {
            continue;
        }
        if sanitized.len().saturating_add(c.len_utf8()) > max_bytes {
            break;
        }
        sanitized.push(c);
    }
    sanitized
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn sanitize_for_display_strips_and_truncates() {
        assert_eq!(
            sanitize_for_display("bad\u{202e}txt\n\u{2028}peer\u{200b}\u{1b}[31m", 100),
            "badtxtpeer[31m"
        );
        // Cut on a character boundary, never inside one
        assert_eq!(sanitize_for_display("ééé", 5), "éé");
        assert_eq!(sanitize_for_display(&"a".repeat(1000), 256).len(), 256);
        assert_eq!(sanitize_for_display("café ü 日本", 256), "café ü 日本");
    }
}
