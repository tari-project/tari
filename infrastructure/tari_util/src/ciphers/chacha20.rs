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

use crate::{
    ciphers::cipher::{Cipher, CipherError},
    ByteArray,
};
use clear_on_drop::clear::Clear;
use rand::{OsRng, RngCore};
/// This in an implementation of the ChaCha20 stream cipher developed using the Internet Research Task Force (IRTF) 8439 RFC (https://tools.ietf.org/html/rfc8439)
/// ChaCha20 is a high-speed cipher proposed by D. Bernstein that is not sensitive to timing attacks (http://cr.yp.to/chacha/chacha-20080128.pdf).
/// Data used in the unit tests were derived from the examples from the IRTF 8439 RFC
use std::num::Wrapping;

pub struct ChaCha20;

impl ChaCha20 {
    /// Perform a single chacha quarter round at the provided indices in the state
    fn quarter_round(state: &mut [u32; 16], a_index: usize, b_index: usize, c_index: usize, d_index: usize) {
        let mut a = Wrapping(state[a_index]);
        let mut b = Wrapping(state[b_index]);
        let mut c = Wrapping(state[c_index]);
        let mut d = Wrapping(state[d_index]);
        a += b;
        d = Wrapping((d ^ a).0.rotate_left(16));
        c += d;
        b = Wrapping((b ^ c).0.rotate_left(12));
        a += b;
        d = Wrapping((d ^ a).0.rotate_left(8));
        c += d;
        b = Wrapping((b ^ c).0.rotate_left(7));
        state[a_index] = a.0;
        state[b_index] = b.0;
        state[c_index] = c.0;
        state[d_index] = d.0;
    }

    /// Construct a chacha block by performing a number of column and diagonal quarter round operations
    fn chacha20_block(state: &[u32; 16]) -> [u32; 16] {
        let mut working_state = *state;
        for _iter in 0..10 {
            // 20 total => odd and even round performed for every iteration
            // Odd round
            Self::quarter_round(&mut working_state, 0, 4, 8, 12);
            Self::quarter_round(&mut working_state, 1, 5, 9, 13);
            Self::quarter_round(&mut working_state, 2, 6, 10, 14);
            Self::quarter_round(&mut working_state, 3, 7, 11, 15);
            // Even round
            Self::quarter_round(&mut working_state, 0, 5, 10, 15);
            Self::quarter_round(&mut working_state, 1, 6, 11, 12);
            Self::quarter_round(&mut working_state, 2, 7, 8, 13);
            Self::quarter_round(&mut working_state, 3, 4, 9, 14);
        }
        let mut output: [u32; 16] = [0; 16];
        for i in 0..output.len() {
            output[i] = (Wrapping(working_state[i]) + Wrapping(state[i])).0;
        }
        (output)
    }

    /// Construct an initial state from a 128-bit constant, 256-bit key, 96-bit nonce and a 32-bit block counter
    #[allow(clippy::needless_range_loop)]
    fn construct_state(key: &[u8; 32], nonce: &[u8; 12], counter: u32) -> [u32; 16] {
        let constant: [u8; 16] = [101, 120, 112, 97, 110, 100, 32, 51, 50, 45, 98, 121, 116, 101, 32, 107]; // 0x61707865, 0x3320646e, 0x79622d32, 0x6b206574
        let mut state_bytes = constant.to_vec(); // 128 bit
        state_bytes.extend_from_slice(key); // 256-bit
        state_bytes.extend_from_slice(&counter.to_ne_bytes()); // 32-bit
        state_bytes.extend_from_slice(nonce); // 96-bit

        // Convert [u8;64] to [u32;16]
        let mut curr_bytes: [u8; 4] = [0; 4];
        let mut state: [u32; 16] = [0; 16];
        for i in 0..state.len() {
            let byte_index = i * 4;
            curr_bytes.copy_from_slice(&state_bytes[byte_index..(byte_index + 4)]);
            state[i] = u32::from_le_bytes(curr_bytes);
        }
        state_bytes.clear();
        (state)
    }

    /// Generate a keystream consisting of a number of chacha blocks
    fn chacha20_cipher_keystream(key: &[u8; 32], nonce: &[u8; 12], block_count: usize) -> Vec<u8> {
        const BYTES_PER_BLOCK: usize = 64;
        let block_byte_count = block_count * BYTES_PER_BLOCK;
        let mut cipher_bytes: Vec<u8> = Vec::with_capacity(block_byte_count as usize);
        for counter in 1..=block_count as u32 {
            let mut state = Self::construct_state(key, nonce, counter);
            let cipher_block = Self::chacha20_block(&state);
            state.clear();
            // convert cipher block to bytes
            let mut block_bytes: Vec<u8> = Vec::with_capacity(BYTES_PER_BLOCK);
            for block in cipher_block.iter() {
                block_bytes.append(&mut block.to_ne_bytes().to_vec())
            }
            cipher_bytes.append(&mut block_bytes)
        }
        (cipher_bytes)
    }

    /// Encode the provided input bytes using a chacha20 keystream
    fn encode_with_nonce(bytes: &[u8], key: &[u8; 32], nonce: &[u8; 12]) -> Vec<u8> {
        const BYTES_PER_BLOCK: usize = 64;
        let block_count = (bytes.len() as f64 / BYTES_PER_BLOCK as f64).ceil() as usize;
        let cipher_bytes = Self::chacha20_cipher_keystream(key, nonce, block_count);
        let mut encoded_bytes = Vec::with_capacity(bytes.len());
        for i in 0..bytes.len() {
            encoded_bytes.push(cipher_bytes[i] ^ bytes[i]);
        }
        (encoded_bytes)
    }

    /// Decode the provided input bytes using a chacha20 keystream
    fn decode_with_nonce(bytes: &[u8], key: &[u8; 32], nonce: &[u8; 12]) -> Vec<u8> {
        (Self::encode_with_nonce(bytes, &key, &nonce))
    }
}

impl<D> Cipher<D> for ChaCha20
where D: ByteArray
{
    fn seal(plain_text: &D, key: &[u8], nonce: &[u8]) -> Result<Vec<u8>, CipherError> {
        // Validation
        if key.len() != 32 {
            return Err(CipherError::KeyLengthError);
        }
        if nonce.len() != 12 {
            return Err(CipherError::NonceLengthError);
        }
        if plain_text.as_bytes().is_empty() {
            return Err(CipherError::NoDataError);
        }

        let mut sized_key = [0; 32];
        sized_key.copy_from_slice(key);

        let mut sized_nonce = [0; 12];
        sized_nonce.copy_from_slice(nonce);

        let cipher_text = ChaCha20::encode_with_nonce(plain_text.as_bytes(), &sized_key, &sized_nonce);
        // Clear copied private data
        sized_key.clear();
        sized_nonce.clear();

        Ok(cipher_text)
    }

    fn open(cipher_text: &[u8], key: &[u8], nonce: &[u8]) -> Result<D, CipherError> {
        // Validation
        if key.len() != 32 {
            return Err(CipherError::KeyLengthError);
        }
        if nonce.len() != 12 {
            return Err(CipherError::NonceLengthError);
        }
        if cipher_text.is_empty() {
            return Err(CipherError::NoDataError);
        }

        let mut sized_key = [0; 32];
        sized_key.copy_from_slice(key);

        let mut sized_nonce = [0; 12];
        sized_nonce.copy_from_slice(nonce);

        let plain_text = ChaCha20::decode_with_nonce(cipher_text, &sized_key, &sized_nonce);
        // Clear copied private data
        sized_key.clear();
        sized_nonce.clear();

        Ok(D::from_vec(&plain_text)?)
    }

    fn seal_with_integral_nonce(plain_text: &D, key: &[u8]) -> Result<Vec<u8>, CipherError> {
        // Validation
        if key.len() != 32 {
            return Err(CipherError::KeyLengthError);
        }
        if plain_text.as_bytes().is_empty() {
            return Err(CipherError::NoDataError);
        }

        let mut sized_key = [0; 32];
        sized_key.copy_from_slice(key);

        let mut rng = OsRng::new().unwrap();
        let mut nonce = [0u8; 12];
        rng.fill_bytes(&mut nonce);

        let cipher_text = ChaCha20::encode_with_nonce(plain_text.as_bytes(), &sized_key, &nonce);
        let mut nonce_with_cipher_text: Vec<u8> = nonce.to_vec();
        nonce_with_cipher_text.extend(cipher_text);

        // Clear copied private data
        sized_key.clear();
        nonce.clear();

        Ok(nonce_with_cipher_text)
    }

    fn open_with_integral_nonce(cipher_text: &[u8], key: &[u8]) -> Result<D, CipherError> {
        // Validation
        if key.len() != 32 {
            return Err(CipherError::KeyLengthError);
        }
        // If the cipher text is shorter than the required nonce length then the nonce is not properly included
        if cipher_text.len() < 12 {
            return Err(CipherError::NonceLengthError);
        } else if cipher_text.len() < 13 {
            return Err(CipherError::NoDataError);
        }

        let mut sized_key = [0; 32];
        sized_key.copy_from_slice(key);

        let mut nonce = [0u8; 12];
        nonce.copy_from_slice(&cipher_text.clone()[0..12]);

        let plain_text = ChaCha20::decode_with_nonce(&cipher_text[12..], &sized_key, &nonce);
        // Clear copied private data
        sized_key.clear();
        nonce.clear();

        Ok(D::from_vec(&plain_text)?)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_quarter_round() {
        // Partial state quarter round update
        let mut state: [u32; 16] = [0; 16];
        state[0] = 286331153;
        state[1] = 16909060;
        state[2] = 2609737539;
        state[3] = 19088743;
        ChaCha20::quarter_round(&mut state, 0, 1, 2, 3);
        assert_eq!(state[0], 3928658676);
        assert_eq!(state[1], 3407673550);
        assert_eq!(state[2], 1166100270);
        assert_eq!(state[3], 1484899515);
        // Full state quarter round update
        let mut state: [u32; 16] = [
            2274701792, 3320640381, 1365533105, 3383111562, 1153568499, 865120127, 3657197835, 710897996, 1396123495,
            2953467441, 2538361882, 899586403, 1553404001, 1029904009, 546888150, 2447102752,
        ];
        let desired_state: [u32; 16] = [
            2274701792, 3320640381, 3182986972, 3383111562, 1153568499, 865120127, 3657197835, 3484200914, 3832277632,
            2953467441, 2538361882, 899586403, 1553404001, 3435166841, 546888150, 2447102752,
        ];
        ChaCha20::quarter_round(&mut state, 2, 7, 8, 13);
        assert_eq!(state, desired_state);
    }

    #[test]
    fn test_init_state() {
        let key: [u8; 32] = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11,
            0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
        ];
        let nonce: [u8; 12] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x4A, 0x00, 0x00, 0x00, 0x00];
        let counter: u32 = 1;
        let state = ChaCha20::construct_state(&key, &nonce, counter);
        let desired_state: [u32; 16] = [
            1634760805, 857760878, 2036477234, 1797285236, 50462976, 117835012, 185207048, 252579084, 319951120,
            387323156, 454695192, 522067228, 1, 0, 1241513984, 0,
        ];
        assert_eq!(state, desired_state);
    }

    #[test]
    fn test_chacha20_block() {
        let state: [u32; 16] = [
            1634760805, 857760878, 2036477234, 1797285236, 50462976, 117835012, 185207048, 252579084, 319951120,
            387323156, 454695192, 522067228, 1, 150994944, 1241513984, 0,
        ];
        let block_state = ChaCha20::chacha20_block(&state);
        let desired_block_state: [u32; 16] = [
            3840405776, 358169553, 534581072, 3295748259, 3354710471, 57196595, 2594841092, 1315755203, 1180992210,
            162176775, 98026004, 2718075865, 3516666549, 3108902622, 3900952779, 1312575650,
        ];
        assert_eq!(block_state, desired_block_state);
    }

    #[test]
    fn test_chacha20_cipher_keystream() {
        let key: [u8; 32] = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11,
            0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
        ];
        let nonce: [u8; 12] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x4A, 0x00, 0x00, 0x00, 0x00];
        let block_count: usize = 1;
        let desired_cipher_bytes: Vec<u8> = vec![
            0x22, 0x4f, 0x51, 0xf3, 0x40, 0x1b, 0xd9, 0xe1, 0x2f, 0xde, 0x27, 0x6f, 0xb8, 0x63, 0x1d, 0xed, 0x8c, 0x13,
            0x1f, 0x82, 0x3d, 0x2c, 0x06, 0xe2, 0x7e, 0x4f, 0xca, 0xec, 0x9e, 0xf3, 0xcf, 0x78, 0x8a, 0x3b, 0x0a, 0xa3,
            0x72, 0x60, 0x0a, 0x92, 0xb5, 0x79, 0x74, 0xcd, 0xed, 0x2b, 0x93, 0x34, 0x79, 0x4c, 0xba, 0x40, 0xc6, 0x3e,
            0x34, 0xcd, 0xea, 0x21, 0x2c, 0x4c, 0xf0, 0x7d, 0x41, 0xb7,
        ];
        let cipher_bytes = ChaCha20::chacha20_cipher_keystream(&key, &nonce, block_count);
        assert_eq!(cipher_bytes, desired_cipher_bytes);
    }

    #[test]
    fn test_encode_and_decode() {
        let key: [u8; 32] = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11,
            0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
        ];

        let nonce: [u8; 12] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x4A, 0x00, 0x00, 0x00, 0x00];
        // Text: "Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen
        // would be it."
        // The test data bytes are from IRTF 8439 RFC
        let data_bytes: Vec<u8> = vec![
            0x4c, 0x61, 0x64, 0x69, 0x65, 0x73, 0x20, 0x61, 0x6e, 0x64, 0x20, 0x47, 0x65, 0x6e, 0x74, 0x6c, 0x65, 0x6d,
            0x65, 0x6e, 0x20, 0x6f, 0x66, 0x20, 0x74, 0x68, 0x65, 0x20, 0x63, 0x6c, 0x61, 0x73, 0x73, 0x20, 0x6f, 0x66,
            0x20, 0x27, 0x39, 0x39, 0x3a, 0x20, 0x49, 0x66, 0x20, 0x49, 0x20, 0x63, 0x6f, 0x75, 0x6c, 0x64, 0x20, 0x6f,
            0x66, 0x66, 0x65, 0x72, 0x20, 0x79, 0x6f, 0x75, 0x20, 0x6f, 0x6e, 0x6c, 0x79, 0x20, 0x6f, 0x6e, 0x65, 0x20,
            0x74, 0x69, 0x70, 0x20, 0x66, 0x6f, 0x72, 0x20, 0x74, 0x68, 0x65, 0x20, 0x66, 0x75, 0x74, 0x75, 0x72, 0x65,
            0x2c, 0x20, 0x73, 0x75, 0x6e, 0x73, 0x63, 0x72, 0x65, 0x65, 0x6e, 0x20, 0x77, 0x6f, 0x75, 0x6c, 0x64, 0x20,
            0x62, 0x65, 0x20, 0x69, 0x74, 0x2e,
        ];
        // Encode
        let encoded_bytes = ChaCha20::encode_with_nonce(&data_bytes, &key, &nonce);
        let desired_encoded_bytes: Vec<u8> = vec![
            0x6e, 0x2e, 0x35, 0x9a, 0x25, 0x68, 0xf9, 0x80, 0x41, 0xba, 0x07, 0x28, 0xdd, 0x0d, 0x69, 0x81, 0xe9, 0x7e,
            0x7a, 0xec, 0x1d, 0x43, 0x60, 0xc2, 0x0a, 0x27, 0xaf, 0xcc, 0xfd, 0x9f, 0xae, 0x0b, 0xf9, 0x1b, 0x65, 0xc5,
            0x52, 0x47, 0x33, 0xab, 0x8f, 0x59, 0x3d, 0xab, 0xcd, 0x62, 0xb3, 0x57, 0x16, 0x39, 0xd6, 0x24, 0xe6, 0x51,
            0x52, 0xab, 0x8f, 0x53, 0x0c, 0x35, 0x9f, 0x08, 0x61, 0xd8, 0x07, 0xca, 0x0d, 0xbf, 0x50, 0x0d, 0x6a, 0x61,
            0x56, 0xa3, 0x8e, 0x08, 0x8a, 0x22, 0xb6, 0x5e, 0x52, 0xbc, 0x51, 0x4d, 0x16, 0xcc, 0xf8, 0x06, 0x81, 0x8c,
            0xe9, 0x1a, 0xb7, 0x79, 0x37, 0x36, 0x5a, 0xf9, 0x0b, 0xbf, 0x74, 0xa3, 0x5b, 0xe6, 0xb4, 0x0b, 0x8e, 0xed,
            0xf2, 0x78, 0x5e, 0x42, 0x87, 0x4d,
        ];
        assert_eq!(encoded_bytes, desired_encoded_bytes);
        // Decode
        let decoded_bytes = ChaCha20::decode_with_nonce(&encoded_bytes, &key, &nonce);
        assert_eq!(decoded_bytes, data_bytes);
    }

    #[test]
    fn test_cipher_trait() {
        let key: Vec<u8> = vec![
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11,
            0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
        ];

        let nonce: Vec<u8> = vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x4A, 0x00, 0x00, 0x00, 0x00];
        // Text: "Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen
        // would be it." The test data bytes are from IRTF 8439 RFC
        let data_bytes: Vec<u8> = vec![
            0x4c, 0x61, 0x64, 0x69, 0x65, 0x73, 0x20, 0x61, 0x6e, 0x64, 0x20, 0x47, 0x65, 0x6e, 0x74, 0x6c, 0x65, 0x6d,
            0x65, 0x6e, 0x20, 0x6f, 0x66, 0x20, 0x74, 0x68, 0x65, 0x20, 0x63, 0x6c, 0x61, 0x73, 0x73, 0x20, 0x6f, 0x66,
            0x20, 0x27, 0x39, 0x39, 0x3a, 0x20, 0x49, 0x66, 0x20, 0x49, 0x20, 0x63, 0x6f, 0x75, 0x6c, 0x64, 0x20, 0x6f,
            0x66, 0x66, 0x65, 0x72, 0x20, 0x79, 0x6f, 0x75, 0x20, 0x6f, 0x6e, 0x6c, 0x79, 0x20, 0x6f, 0x6e, 0x65, 0x20,
            0x74, 0x69, 0x70, 0x20, 0x66, 0x6f, 0x72, 0x20, 0x74, 0x68, 0x65, 0x20, 0x66, 0x75, 0x74, 0x75, 0x72, 0x65,
            0x2c, 0x20, 0x73, 0x75, 0x6e, 0x73, 0x63, 0x72, 0x65, 0x65, 0x6e, 0x20, 0x77, 0x6f, 0x75, 0x6c, 0x64, 0x20,
            0x62, 0x65, 0x20, 0x69, 0x74, 0x2e,
        ];

        let desired_encoded_bytes: Vec<u8> = vec![
            0x6e, 0x2e, 0x35, 0x9a, 0x25, 0x68, 0xf9, 0x80, 0x41, 0xba, 0x07, 0x28, 0xdd, 0x0d, 0x69, 0x81, 0xe9, 0x7e,
            0x7a, 0xec, 0x1d, 0x43, 0x60, 0xc2, 0x0a, 0x27, 0xaf, 0xcc, 0xfd, 0x9f, 0xae, 0x0b, 0xf9, 0x1b, 0x65, 0xc5,
            0x52, 0x47, 0x33, 0xab, 0x8f, 0x59, 0x3d, 0xab, 0xcd, 0x62, 0xb3, 0x57, 0x16, 0x39, 0xd6, 0x24, 0xe6, 0x51,
            0x52, 0xab, 0x8f, 0x53, 0x0c, 0x35, 0x9f, 0x08, 0x61, 0xd8, 0x07, 0xca, 0x0d, 0xbf, 0x50, 0x0d, 0x6a, 0x61,
            0x56, 0xa3, 0x8e, 0x08, 0x8a, 0x22, 0xb6, 0x5e, 0x52, 0xbc, 0x51, 0x4d, 0x16, 0xcc, 0xf8, 0x06, 0x81, 0x8c,
            0xe9, 0x1a, 0xb7, 0x79, 0x37, 0x36, 0x5a, 0xf9, 0x0b, 0xbf, 0x74, 0xa3, 0x5b, 0xe6, 0xb4, 0x0b, 0x8e, 0xed,
            0xf2, 0x78, 0x5e, 0x42, 0x87, 0x4d,
        ];

        assert_eq!(
            ChaCha20::seal(&data_bytes, &key[..31].to_vec(), &nonce),
            Err(CipherError::KeyLengthError)
        );

        assert_eq!(
            ChaCha20::seal(&data_bytes, &key, &nonce[..11].to_vec()),
            Err(CipherError::NonceLengthError)
        );

        let cipher_text = ChaCha20::seal(&data_bytes, &key, &nonce).unwrap();

        assert_eq!(cipher_text, desired_encoded_bytes);

        let plain_text: Vec<u8> = ChaCha20::open(&cipher_text, &key, &nonce).unwrap();

        assert_eq!(plain_text, data_bytes);
    }

    #[test]
    fn test_integral_nonce_cipher() {
        let key: Vec<u8> = vec![
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11,
            0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
        ];
        let data_bytes: Vec<u8> = "One Ring to rule them all, One Ring to find them, One Ring to bring them all, and \
                                   in the darkness bind them"
            .as_bytes()
            .to_vec();

        let cipher_text = ChaCha20::seal_with_integral_nonce(&data_bytes, &key).unwrap();
        let plain_text: Vec<u8> = ChaCha20::open_with_integral_nonce(&cipher_text, &key).unwrap();

        assert_eq!(plain_text, data_bytes);
    }
}
