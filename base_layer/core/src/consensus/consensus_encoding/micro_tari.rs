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

use std::{
    io::{Error, Read, Write},
    mem,
};

use crate::{
    consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized},
    transactions::tari_amount::MicroTari,
};

const U64_SIZE: usize = mem::size_of::<u64>();

impl ConsensusEncoding for MicroTari {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<usize, Error> {
        writer.write_all(&self.0.to_le_bytes()[..])?;
        Ok(U64_SIZE)
    }
}

impl ConsensusEncodingSized for MicroTari {
    fn consensus_encode_exact_size(&self) -> usize {
        U64_SIZE
    }
}

impl ConsensusDecoding for MicroTari {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let mut buf = [0u8; U64_SIZE];
        reader.read_exact(&mut buf)?;
        Ok(u64::from_le_bytes(buf).into())
    }
}
