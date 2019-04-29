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

use blake2::VarBlake2b;
use digest::{
    generic_array::{typenum::U32, GenericArray},
    FixedOutput,
    Input,
    Reset,
    VariableOutput,
};

/// A convenience wrapper produce 256 bit hashes from Blake2b
#[derive(Clone, Debug)]
pub struct Blake256(VarBlake2b);

impl Blake256 {
    pub fn new() -> Self {
        let h = VarBlake2b::new(32).unwrap();
        Blake256(h)
    }

    pub fn result(self) -> GenericArray<u8, U32> {
        self.fixed_result()
    }
}

impl Default for Blake256 {
    fn default() -> Self {
        let h = VarBlake2b::new(32).unwrap();
        Blake256(h)
    }
}

impl Input for Blake256 {
    fn input<B: AsRef<[u8]>>(&mut self, data: B) {
        (self.0).input(data);
    }
}

impl FixedOutput for Blake256 {
    type OutputSize = U32;

    fn fixed_result(self) -> GenericArray<u8, U32> {
        let v = (self.0).vec_result();
        GenericArray::clone_from_slice(&v)
    }
}

impl Reset for Blake256 {
    fn reset(&mut self) {
        (self.0).reset()
    }
}

#[cfg(test)]
mod test {
    use crate::common::Blake256;
    use digest::Input;
    use tari_utilities::hex;

    #[test]
    fn blake256() {
        let e = Blake256::new().chain(b"one").chain(b"two").result().to_vec();
        let h = hex::to_hex(&e);
        assert_eq!(
            h,
            "03521c1777639fc6e5c3d8c3b4600870f18becc155ad7f8053d2c65bc78e4aa0".to_string()
        );
    }
}
