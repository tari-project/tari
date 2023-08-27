//  Copyright 2021, The Taiji Project
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

/// Return a currency styled `String`
/// # Examples
///
/// ```rust
/// use taiji_core::transactions::format_currency;
/// assert_eq!("12,345.12", format_currency("12345.12", ','));
/// assert_eq!("12,345", format_currency("12345", ','));
/// ```
pub fn format_currency(value: &str, separator: char) -> String {
    let full_len = value.len();
    let mut buffer = String::with_capacity(full_len / 3 + full_len);
    let mut iter = value.splitn(2, '.');
    let whole = iter.next().unwrap_or("");
    for (i, c) in whole.chars().enumerate() {
        buffer.push(c);
        let idx = whole.len() - i - 1;
        if idx > 0 && idx % 3 == 0 {
            buffer.push(separator);
        }
    }
    if let Some(decimal) = iter.next() {
        buffer.push('.');
        buffer.push_str(decimal);
    }
    buffer
}

#[cfg(test)]
#[allow(clippy::excessive_precision)]
mod test {
    use super::format_currency;

    #[test]
    fn test_format_currency() {
        assert_eq!("0.00", format_currency("0.00", ','));
        assert_eq!("0.000000000000", format_currency("0.000000000000", ','));
        assert_eq!("123,456.123456789", format_currency("123456.123456789", ','));
        assert_eq!("1,123,123,456.123456789", format_currency("1123123456.123456789", ','));
        assert_eq!("123,456", format_currency("123456", ','));
        assert_eq!("123", format_currency("123", ','));
        assert_eq!("7,123", format_currency("7123", ','));
        assert_eq!(".00", format_currency(".00", ','));
        assert_eq!("00.", format_currency("00.", ','));
    }
}
