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

use crate::chacha20::*;

pub trait Cipher {
    /// Encode using a cipher keystream with a default nonce
    fn encode(&self, key: &[u8; 32]) -> Vec<u8>;

    /// Encode using a cipher keystream with a self selected nonce
    fn encode_with_nonce(&self, key: &[u8; 32], nonce: &[u32; 3]) -> Vec<u8>;

    /// Decode using a cipher keystream with a default nonce
    fn decode(bytes: &[u8], key: &[u8; 32]) -> Self
    where Self: Sized;

    /// Decode using a cipher keystream with a self selected nonce
    fn decode_with_nonce(bytes: &[u8], key: &[u8; 32], nonce: &[u32; 3]) -> Self
    where Self: Sized;
}

impl Cipher for Vec<u8> {
    fn encode(&self, key: &[u8; 32]) -> Vec<u8> {
        (encode(self, key))
    }

    fn encode_with_nonce(&self, key: &[u8; 32], nonce: &[u32; 3]) -> Vec<u8> {
        (encode_with_nonce(self, &key, &nonce))
    }

    fn decode(bytes: &[u8], key: &[u8; 32]) -> Self {
        (decode(&bytes, &key))
    }

    fn decode_with_nonce(bytes: &[u8], key: &[u8; 32], nonce: &[u32; 3]) -> Self {
        (decode_with_nonce(&bytes, &key, &nonce))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_cipher_encode_and_decode() {
        let key: [u8; 32] = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11,
            0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
        ];
        let data_bytes: Vec<u8> = "One Ring to rule them all, One Ring to find them, One Ring to bring them all, and \
                                   in the darkness bind them"
            .as_bytes()
            .to_vec();
        let encoded_bytes = data_bytes.encode(&key);
        let decoded_bytes: Vec<u8> = Cipher::decode(&encoded_bytes, &key);
        assert_ne!(data_bytes, encoded_bytes);
        assert_eq!(data_bytes, decoded_bytes);
    }
}
