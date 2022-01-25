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
    io,
    io::{Read, Write},
};

use tari_crypto::script::{ExecutionStack, TariScript};

use crate::consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized, MaxSizeBytes};

impl ConsensusEncoding for TariScript {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
        self.as_bytes().consensus_encode(writer)
    }
}

/// TODO: implement zero-alloc ConsensusEncodingSized for TariScript
impl ConsensusEncodingSized for TariScript {}

impl ConsensusDecoding for TariScript {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        const MAX_SCRIPT_SIZE: usize = 4096;
        let script_bytes = MaxSizeBytes::<MAX_SCRIPT_SIZE>::consensus_decode(reader)?;
        let script = TariScript::from_bytes(&script_bytes).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Failed to deserialize bytes: {}", err),
            )
        })?;
        Ok(script)
    }
}

impl ConsensusEncoding for ExecutionStack {
    fn consensus_encode<W: io::Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
        self.as_bytes().consensus_encode(writer)
    }
}

impl ConsensusEncodingSized for ExecutionStack {}

impl ConsensusDecoding for ExecutionStack {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        const MAX_STACK_SIZE: usize = 4096;
        let bytes = MaxSizeBytes::<MAX_STACK_SIZE>::consensus_decode(reader)?;
        let stack =
            ExecutionStack::from_bytes(&bytes).map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
        Ok(stack)
    }
}
