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

// This file only contains hashvalue lookups for the mmr tests. The values are stored in an array representing the same
// storage of the mmr. All values are Hex encoded
use blake2::Blake2b;
use digest::Digest;
use std::{fmt::Write, num::ParseIntError};

pub struct HashValues {
    values: Vec<String>,
}

impl HashValues {
    pub fn get_value(&self, index: usize) -> String {
        self.values[index].clone()
    }

    pub fn get_slice(&self, end: usize) -> Vec<String> {
        let mut result = Vec::new();
        result.resize(end + 1, "".to_string());
        result[..end + 1].clone_from_slice(&(self.values[..end + 1]));
        result
    }

    pub fn new() -> HashValues {
        let mut hashvalues = HashValues { values: Vec::new() };
        hashvalues.values.push("1ced8f5be2db23a6513eba4d819c73806424748a7bc6fa0d792cc1c7d1775a9778e894aa91413f6eb79ad5ae2f871eafcc78797e4c82af6d1cbfb1a294a10d10".to_string());
        hashvalues.values.push("c5faca15ac2f93578b39ef4b6bbb871bdedce4ddd584fd31f0bb66fade3947e6bb1353e562414ed50638a8829ff3daccac7ef4a50acee72a5384ba9aeb604fc9".to_string());
        hashvalues.values.push("4d3d9d4c8da746e2dcf236f31b53850e0e35a07c1d6082be51b33e7c1e11c39cf5e309953bf56866b0ccede95cdf3ae5f9823f6cf3bcc6ada19cf21b09884717".to_string());
        // 4
        hashvalues
    }

    pub fn to_hex(bytes: &Vec<u8>) -> String {
        let mut s = String::new();
        for byte in bytes {
            write!(&mut s, "{:02x}", byte).expect("Unable to write");
        }
        s
    }

    pub fn to_hex_multiple(bytearray: &Vec<Vec<u8>>) -> Vec<String> {
        let mut result = Vec::new();
        for bytes in bytearray {
            result.push(HashValues::to_hex(bytes))
        }
        result
    }

    pub fn get_hash_in_hex<D: Digest>(hash1: &Vec<u8>, hash2: &Vec<u8>) -> String {
        let mut hasher = D::new();
        hasher.input(hash1);
        hasher.input(hash2);
        let new_hash = hasher.result().to_vec();
        HashValues::to_hex(&new_hash)
    }
}
