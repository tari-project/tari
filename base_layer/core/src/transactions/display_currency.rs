//  Copyright 2021, The Tari Project
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
/// ```
/// use tari_core::transactions::display_currency::display_currency;
/// assert_eq!(String::from("12,345.12"), display_currency(12345.12, 2, ","));
/// assert_eq!(String::from("12,345"), display_currency(12345.12, 0, ","));
/// ```
pub fn display_currency(value: f64, precision: usize, separator: &str) -> String {
    let whole = value as usize;
    let decimal = ((value - whole as f64) * num::pow(10_f64, precision)).round() as usize;
    let formatted_whole_value = whole
        .to_string()
        .chars()
        .rev()
        .enumerate()
        .fold(String::new(), |acc, (i, c)| {
            if i != 0 && i % 3 == 0 {
                format!("{}{}{}", acc, separator, c)
            } else {
                format!("{}{}", acc, c)
            }
        })
        .chars()
        .rev()
        .collect::<String>();

    if precision > 0 {
        format!("{}.{:0>2$}", formatted_whole_value, decimal, precision)
    } else {
        formatted_whole_value
    }
}

#[cfg(test)]
#[allow(clippy::excessive_precision)]
mod test {
    #[test]
    fn display_currency() {
        assert_eq!(String::from("0.00"), super::display_currency(0.0f64, 2, ","));
        assert_eq!(String::from("0.000000000000"), super::display_currency(0.0f64, 12, ","));
        assert_eq!(
            String::from("123,456.123456789"),
            super::display_currency(123_456.123_456_789_012_f64, 9, ",")
        );
        assert_eq!(
            String::from("123,456"),
            super::display_currency(123_456.123_456_789_012_f64, 0, ",")
        );
        assert_eq!(String::from("1,234"), super::display_currency(1234.1f64, 0, ","));
    }
}
