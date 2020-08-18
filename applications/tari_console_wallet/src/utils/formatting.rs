// Copyright 2020. The Tari Project
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

use unicode_segmentation::UnicodeSegmentation;

/// Utility function to only display the first and last N characters of a long string. This function is aware of unicode
/// graphemes
pub fn display_compressed_string(string: String, len_first: usize, len_last: usize) -> String {
    let graphemes = UnicodeSegmentation::graphemes(string.as_str(), true).collect::<Vec<&str>>();
    if len_first + len_last >= graphemes.len() {
        return string;
    }

    let mut result = "".to_string();
    for i in graphemes.iter().take(len_first) {
        result.push_str(i);
    }
    result.push_str("...");
    for i in graphemes.iter().skip(graphemes.len() - len_last) {
        result.push_str(i);
    }
    result
}

#[cfg(test)]
mod test {
    use crate::utils::formatting::display_compressed_string;

    #[test]
    fn test_compress_string() {
        let short_str = "testing".to_string();
        assert_eq!(display_compressed_string(short_str.clone(), 5, 5), short_str);
        let long_str = "abcdefghijklmnopqrstuvwxyz".to_string();
        assert_eq!(display_compressed_string(long_str, 3, 3), "abc...xyz".to_string());
        let emoji_str = "🐾💎🎤🎨📌🍄🎰🍉🚧💉💡👟🚒📌🔌🐶🐾🐢🔭🐨😻💨🐎🐊🚢👟🚧🐞🚜🌂🎩🎱📈".to_string();
        assert_eq!(
            display_compressed_string(emoji_str, 3, 6),
            "🐾💎🎤...🐞🚜🌂🎩🎱📈".to_string()
        );
    }
}
