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

/// this trait allows us to call get_raw_bytes and get the raw bytes of the type
pub trait ToBytes {
    fn get_raw_bytes(&self, buf: &mut Vec<u8>);
}

impl<T> ToBytes for Vec<T>
where T: ToBytes
{
    fn get_raw_bytes(&self, buf: &mut Vec<u8>) {
        for t in self {
            t.get_raw_bytes(buf);
        }
    }
}

impl<T> ToBytes for [T]
where T: ToBytes
{
    fn get_raw_bytes(&self, buf: &mut Vec<u8>) {
        for t in self {
            t.get_raw_bytes(buf);
        }
    }
}

impl ToBytes for str {
    fn get_raw_bytes(&self, buf: &mut Vec<u8>) {
        buf.extend(self.as_bytes())
    }
}

impl ToBytes for &str {
    fn get_raw_bytes(&self, buf: &mut Vec<u8>) {
        buf.extend(self.as_bytes())
    }
}

impl ToBytes for String {
    fn get_raw_bytes(&self, buf: &mut Vec<u8>) {
        buf.extend(self.as_bytes())
    }
}

impl ToBytes for i8 {
    fn get_raw_bytes(&self, buf: &mut Vec<u8>) {
        let bytes = self.to_le_bytes();
        buf.extend_from_slice(&bytes);
    }
}
impl ToBytes for i16 {
    fn get_raw_bytes(&self, buf: &mut Vec<u8>) {
        let bytes = self.to_le_bytes();
        buf.extend_from_slice(&bytes);
    }
}
impl ToBytes for i32 {
    fn get_raw_bytes(&self, buf: &mut Vec<u8>) {
        let bytes = self.to_le_bytes();
        buf.extend_from_slice(&bytes);
    }
}
impl ToBytes for i128 {
    fn get_raw_bytes(&self, buf: &mut Vec<u8>) {
        let bytes = self.to_le_bytes();
        buf.extend_from_slice(&bytes);
    }
}

impl ToBytes for u8 {
    fn get_raw_bytes(&self, buf: &mut Vec<u8>) {
        let bytes = self.to_le_bytes();
        buf.extend_from_slice(&bytes);
    }
}
impl ToBytes for u16 {
    fn get_raw_bytes(&self, buf: &mut Vec<u8>) {
        let bytes = self.to_le_bytes();
        buf.extend_from_slice(&bytes);
    }
}
impl ToBytes for u32 {
    fn get_raw_bytes(&self, buf: &mut Vec<u8>) {
        let bytes = self.to_le_bytes();
        buf.extend_from_slice(&bytes);
    }
}
impl ToBytes for u128 {
    fn get_raw_bytes(&self, buf: &mut Vec<u8>) {
        let bytes = self.to_le_bytes();
        buf.extend_from_slice(&bytes);
    }
}
