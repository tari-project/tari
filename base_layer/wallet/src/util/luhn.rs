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

/// Calculates a checksum using the [Luhn mod n algorithm](https://en.wikipedia.org/wiki/Luhn_mod_N_algorithm). The
/// input to the function is an array of indices, each of which is strictly less than `dict_len`, and the size of the
/// dictionary (`dict_len`). The result is the checksum character, also strictly less than `dict_len`.
pub fn checksum(arr: &[usize], dict_len: usize) -> usize {
    // Starting from the right and working leftwards is easier since
    let (sum, _) = arr.iter().rev().fold((0usize, 2usize), |(sum, factor), digit| {
        let mut addend = factor * *digit;
        let factor = factor ^ 3; // Toggles between 1 and 2
        addend = (addend / dict_len) + addend % dict_len;
        (sum + addend, factor)
    });
    (dict_len - (sum % dict_len)) % dict_len
}

/// Checks whether the last digit in the array matches the checksum for the array minus the last digit.
pub fn is_valid(arr: &[usize], dict_len: usize) -> bool {
    let cs = checksum(&arr[..arr.len() - 1], dict_len);
    cs == arr[arr.len() - 1]
}

#[cfg(test)]
mod test {
    use crate::util::luhn::*;

    #[test]
    fn luhn_6() {
        assert_eq!(checksum(&[0, 1, 2, 3, 4, 5], 6), 4);
        for i in 0..6 {
            let valid = is_valid(&[0, 1, 2, 3, 4, 5, i], 6);
            match i {
                4 => assert!(valid),
                _ => assert_eq!(valid, false),
            }
        }
    }

    #[test]
    fn luhn_10() {
        assert_eq!(checksum(&[7, 9, 9, 2, 7, 3, 9, 8, 7, 1], 10), 3);
        for i in 0..10 {
            let valid = is_valid(&[7, 9, 9, 2, 7, 3, 9, 8, 7, 1, i], 10);
            match i {
                3 => assert!(valid),
                _ => assert_eq!(valid, false),
            }
        }
        assert_eq!(checksum(&[1, 0, 4], 10), 0);
        assert_eq!(checksum(&[9, 1, 2, 4, 3, 4, 3, 3, 0], 10), 3);
        assert!(is_valid(&[9, 1, 2, 4, 3, 4, 3, 3, 0, 3], 10));
        // It doesn't catch some transpose errors
        assert!(is_valid(&[0, 1, 2, 4, 3, 4, 3, 3, 9, 3], 10));
    }
}
