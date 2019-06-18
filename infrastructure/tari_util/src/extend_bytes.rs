// Copyright 2019 The Tari Project
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

#[cfg(feature = "chrono_dt")]
use chrono::{DateTime, Utc};

/// this trait allows us to call append_raw_bytes and get the raw bytes of the type
pub trait ExtendBytes {
    fn append_raw_bytes(&self, buf: &mut Vec<u8>);
}

impl<T> ExtendBytes for Vec<T>
where T: ExtendBytes
{
    fn append_raw_bytes(&self, buf: &mut Vec<u8>) {
        for t in self {
            t.append_raw_bytes(buf);
        }
    }
}

impl<T> ExtendBytes for [T]
where T: ExtendBytes
{
    fn append_raw_bytes(&self, buf: &mut Vec<u8>) {
        for t in self {
            t.append_raw_bytes(buf);
        }
    }
}

impl ExtendBytes for str {
    fn append_raw_bytes(&self, buf: &mut Vec<u8>) {
        buf.extend(self.as_bytes())
    }
}

impl ExtendBytes for &str {
    fn append_raw_bytes(&self, buf: &mut Vec<u8>) {
        buf.extend(self.as_bytes())
    }
}

impl ExtendBytes for String {
    fn append_raw_bytes(&self, buf: &mut Vec<u8>) {
        buf.extend(self.as_bytes())
    }
}

impl ExtendBytes for i8 {
    fn append_raw_bytes(&self, buf: &mut Vec<u8>) {
        let bytes = self.to_le_bytes();
        buf.extend_from_slice(&bytes);
    }
}
impl ExtendBytes for i16 {
    fn append_raw_bytes(&self, buf: &mut Vec<u8>) {
        let bytes = self.to_le_bytes();
        buf.extend_from_slice(&bytes);
    }
}
impl ExtendBytes for i32 {
    fn append_raw_bytes(&self, buf: &mut Vec<u8>) {
        let bytes = self.to_le_bytes();
        buf.extend_from_slice(&bytes);
    }
}

impl ExtendBytes for i128 {
    fn append_raw_bytes(&self, buf: &mut Vec<u8>) {
        let bytes = self.to_le_bytes();
        buf.extend_from_slice(&bytes);
    }
}

impl ExtendBytes for u8 {
    fn append_raw_bytes(&self, buf: &mut Vec<u8>) {
        let bytes = self.to_le_bytes();
        buf.extend_from_slice(&bytes);
    }
}
impl ExtendBytes for u16 {
    fn append_raw_bytes(&self, buf: &mut Vec<u8>) {
        let bytes = self.to_le_bytes();
        buf.extend_from_slice(&bytes);
    }
}
impl ExtendBytes for u32 {
    fn append_raw_bytes(&self, buf: &mut Vec<u8>) {
        let bytes = self.to_le_bytes();
        buf.extend_from_slice(&bytes);
    }
}

impl ExtendBytes for u64 {
    fn append_raw_bytes(&self, buf: &mut Vec<u8>) {
        let bytes = self.to_le_bytes();
        buf.extend_from_slice(&bytes);
    }
}

impl ExtendBytes for u128 {
    fn append_raw_bytes(&self, buf: &mut Vec<u8>) {
        let bytes = self.to_le_bytes();
        buf.extend_from_slice(&bytes);
    }
}

impl ExtendBytes for bool {
    fn append_raw_bytes(&self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(if *self { &[1u8] } else { &[0u8] });
    }
}

#[cfg(feature = "chrono_dt")]
impl ExtendBytes for DateTime<Utc> {
    fn append_raw_bytes(&self, buf: &mut Vec<u8>) {
        let bytes = self.timestamp().to_le_bytes();
        buf.extend_from_slice(&bytes);
    }
}
