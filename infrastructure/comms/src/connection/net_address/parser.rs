//  Copyright 2019 The Tari Project
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

use super::{I2PAddress, OnionAddress};

/// Provides simple parsing functionality for address strings.
/// Currently, it contains parsing implementations for Onion and I2P.
pub(crate) struct AddressParser<'a> {
    pos: usize,
    data: &'a [u8],
}

impl<'a> AddressParser<'a> {
    /// Create a new address parser
    pub fn new(s: &'a str) -> Self {
        AddressParser { pos: 0, data: s.as_bytes() }
    }

    /// Parse I2P address
    pub fn parse_i2p(&mut self) -> Option<I2PAddress> {
        self.read_atomic(|p| {
            let name = match p.read_base32_string() {
                Some(n) => {
                    if n.len() != 52 {
                        return None;
                    }
                    n
                },
                None => return None,
            };

            match p.read_until_end() {
                Some(s) => {
                    if s.to_ascii_lowercase() != b".b32.i2p" {
                        return None;
                    }
                },
                None => return None,
            }

            Some(I2PAddress { name })
        })
    }

    /// Parse Onion address
    pub fn parse_onion(&mut self) -> Option<OnionAddress> {
        self.read_atomic(|p| {
            let public_key = match p.read_base32_string() {
                Some(pk) => {
                    // Valid onion address lengths
                    if pk.len() != 16 && pk.len() != 56 {
                        return None;
                    }

                    pk
                },
                _ => return None,
            };

            match p.read_until_char(':') {
                Some(buf) => {
                    if buf.to_ascii_lowercase() != b".onion" {
                        return None;
                    }
                },
                None => return None,
            }

            if p.consume_char(':').is_none() {
                return None;
            }

            let port = match p.read_number() {
                Some(p) => p,
                None => return None,
            };

            if port > std::u16::MAX as u64 {
                return None;
            }

            Some(OnionAddress { public_key, port: port as u16 })
        })
    }

    fn is_base32_char(&self, ch: char) -> bool {
        if ch >= 'A' && ch <= 'Z' {
            return true;
        }

        if ch >= '2' && ch <= '7' {
            return true;
        }

        false
    }

    fn read_base32_string(&mut self) -> Option<String> {
        let mut buf = vec![];
        while self.pos < self.data.len() {
            let ch = self.data[self.pos].to_ascii_uppercase();
            if !self.is_base32_char(ch as char) {
                break;
            }
            buf.push(ch);
            self.pos += 1;
        }

        if buf.len() > 0 {
            match String::from_utf8(buf) {
                Ok(s) => Some(s),
                Err(_) => None,
            }
        } else {
            None
        }
    }

    fn read_char(&mut self) -> Option<char> {
        if self.is_end() {
            return None;
        } else {
            let ch = self.data[self.pos];
            self.pos += 1;
            Some(ch as char)
        }
    }

    fn is_end(&self) -> bool {
        self.pos == self.data.len()
    }

    fn consume_char(&mut self, ch: char) -> Option<char> {
        self.read_char().and_then(|c| if c == ch { Some(ch) } else { None })
    }

    fn read_number(&mut self) -> Option<u64> {
        let mut pos = self.pos;
        let mut number = 0u64;
        while pos < self.data.len() {
            let ch = self.data[pos];

            if ch < b'0' || ch > b'9' {
                break;
            }
            number = number * 10u64 + (ch - b'0') as u64;
            pos += 1;
        }

        if pos == self.pos {
            None
        } else {
            self.pos = pos;
            Some(number)
        }
    }

    fn read_until_end(&mut self) -> Option<Vec<u8>> {
        let buf = &self.data[self.pos..];
        self.pos = self.data.len() - 1;
        Some(buf.to_vec())
    }

    fn read_until_char(&mut self, ch: char) -> Option<Vec<u8>> {
        let mut pos = self.pos;
        let mut buf = vec![];
        while pos < self.data.len() {
            if self.data[pos] == (ch as u8) {
                self.pos = pos;
                return Some(buf);
            }
            buf.push(self.data[pos]);
            pos += 1;
        }

        None
    }

    fn read_atomic<T, F>(&mut self, f: F) -> Option<T>
    where F: FnOnce(&mut Self) -> Option<T> {
        let pos = self.pos;
        let result = f(self);
        if result.is_none() {
            self.pos = pos;
        }
        result
    }
}
