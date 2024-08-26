//  Copyright 2022. The Tari Project
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

use std::{convert::TryFrom, fmt::Display};

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// A string that can only be a up to MAX length long
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct MaxSizeString<const MAX: usize> {
    string: String,
}

impl<const MAX: usize> MaxSizeString<MAX> {
    pub fn from_str_checked(s: &str) -> Option<Self> {
        if s.len() > MAX {
            return None;
        }
        Some(Self { string: s.to_string() })
    }

    pub fn from_utf8_bytes_checked<T: AsRef<[u8]>>(bytes: T) -> Option<Self> {
        let b = bytes.as_ref();
        if b.len() > MAX {
            return None;
        }

        let s = String::from_utf8(b.to_vec()).ok()?;
        Some(Self { string: s })
    }

    pub fn len(&self) -> usize {
        self.string.len()
    }

    pub fn is_empty(&self) -> bool {
        self.string.is_empty()
    }

    pub fn as_str(&self) -> &str {
        &self.string
    }

    pub fn into_string(self) -> String {
        self.string
    }
}

impl<const MAX: usize> TryFrom<String> for MaxSizeString<MAX> {
    type Error = MaxSizeStringLengthError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.len() > MAX {
            return Err(MaxSizeStringLengthError {
                actual: value.len(),
                expected: MAX,
            });
        }
        Ok(Self { string: value })
    }
}

impl<const MAX: usize> TryFrom<&str> for MaxSizeString<MAX> {
    type Error = MaxSizeStringLengthError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value.len() > MAX {
            return Err(MaxSizeStringLengthError {
                actual: value.len(),
                expected: MAX,
            });
        }
        Ok(Self {
            string: value.to_string(),
        })
    }
}

impl<const MAX: usize> AsRef<[u8]> for MaxSizeString<MAX> {
    fn as_ref(&self) -> &[u8] {
        self.string.as_ref()
    }
}

impl<const MAX: usize> Display for MaxSizeString<MAX> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.string)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid String length: expected {expected}, got {actual}")]
pub struct MaxSizeStringLengthError {
    expected: usize,
    actual: usize,
}

#[cfg(test)]
mod tests {
    mod from_str_checked {
        use crate::MaxSizeString;
        #[test]
        fn it_returns_none_if_size_exceeded() {
            let s = MaxSizeString::<10>::from_str_checked("12345678901234567890");
            assert_eq!(s, None);
        }

        #[test]
        fn it_returns_some_if_size_in_bounds() {
            let s = MaxSizeString::<0>::from_str_checked("").unwrap();
            assert_eq!(s.as_str(), "");
            assert_eq!(s.len(), 0);

            let s = MaxSizeString::<10>::from_str_checked("1234567890").unwrap();
            assert_eq!(s.as_str(), "1234567890");
            assert_eq!(s.len(), 10);

            let s = MaxSizeString::<10>::from_str_checked("1234").unwrap();
            assert_eq!(s.as_str(), "1234");
            assert_eq!(s.len(), 4);

            let s = MaxSizeString::<8>::from_str_checked("🚀🚀").unwrap();
            assert_eq!(s.as_str(), "🚀🚀");
            // 8 here because an emoji char take 4 bytes each
            assert_eq!(s.len(), 8);
        }
    }

    mod from_utf8_bytes_checked {
        use crate::MaxSizeString;
        #[test]
        fn it_returns_none_if_size_exceeded() {
            let s = MaxSizeString::<10>::from_utf8_bytes_checked([0u8; 11]);
            assert_eq!(s, None);
        }

        #[test]
        fn it_returns_some_if_size_in_bounds() {
            let s = MaxSizeString::<12>::from_utf8_bytes_checked("💡🧭🛖".as_bytes()).unwrap();
            assert_eq!(s.as_str(), "💡🧭🛖");
            assert_eq!(s.len(), 12);
        }

        #[test]
        fn it_returns_none_if_invalid_utf8() {
            let s = MaxSizeString::<10>::from_utf8_bytes_checked([255u8; 10]);
            assert_eq!(s, None);
        }
    }
}
