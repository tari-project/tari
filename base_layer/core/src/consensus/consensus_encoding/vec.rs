//  Copyright 2022, The Tari Project
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

use std::{convert::TryFrom, io, io::Read};

use integer_encoding::VarIntReader;

use crate::consensus::ConsensusDecoding;

pub struct MaxSizeVec<T, const MAX: usize> {
    inner: Vec<T>,
}

impl<T, const MAX: usize> From<MaxSizeVec<T, MAX>> for Vec<T> {
    fn from(value: MaxSizeVec<T, MAX>) -> Self {
        value.inner
    }
}

impl<T, const MAX: usize> TryFrom<Vec<T>> for MaxSizeVec<T, MAX> {
    type Error = Vec<T>;

    fn try_from(value: Vec<T>) -> Result<Self, Self::Error> {
        if value.len() > MAX {
            return Err(value);
        }

        Ok(Self { inner: value })
    }
}

impl<T: ConsensusDecoding, const MAX: usize> ConsensusDecoding for MaxSizeVec<T, MAX> {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let len = reader.read_varint()?;
        if len > MAX {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Vec size ({}) exceeded maximum ({})", len, MAX),
            ));
        }
        let mut elems = Vec::with_capacity(len);
        for _ in 0..len {
            let elem = T::consensus_decode(reader)?;
            elems.push(elem)
        }
        Ok(Self { inner: elems })
    }
}
