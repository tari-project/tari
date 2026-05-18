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

use std::io::Result;

use borsh::{BorshDeserialize, BorshSerialize};

use super::byte_counter::ByteCounter;

pub trait FromBytes<T: BorshDeserialize> {
    fn borsh_from_bytes(buf: &mut &[u8]) -> Result<T>;
}

impl<T: BorshDeserialize> FromBytes<T> for T {
    fn borsh_from_bytes(buf: &mut &[u8]) -> Result<T> {
        T::deserialize(buf)
    }
}

pub trait SerializedSize {
    fn get_serialized_size(&self) -> Result<usize>;
}

impl<T: BorshSerialize> SerializedSize for T {
    fn get_serialized_size(&self) -> Result<usize> {
        let mut counter = ByteCounter::new();
        // The [ByteCounter] never throws an error. But be aware that we can introduce an Error into custom serialize.
        // e.g. MoneroPowData is using external functions that can throw an error. But we don't use this function for
        // it.
        self.serialize(&mut counter)?;
        Ok(counter.get())
    }
}
