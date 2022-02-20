// Copyright 2018 The Tari Project
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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE
//
// Portions of this file were originally copyrighted (c) 2018 The Grin Developers, issued under the Apache License,
// Version 2.0, available at http://www.apache.org/licenses/LICENSE-2.0.

use std::{io, io::Read};

use serde::{Deserialize, Serialize};

use crate::consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized};

bitflags! {
    #[derive(Deserialize, Serialize)]
    pub struct OutputFlags: u8 {
        /// Output is a coinbase output, must not be spent until maturity
        const COINBASE_OUTPUT      = 0b0000_0001;
        const NON_FUNGIBLE         = 0b0000_1000;
        const ASSET_REGISTRATION   = 0b0000_0010 | Self::NON_FUNGIBLE.bits;
        const MINT_NON_FUNGIBLE    = 0b0000_0100 | Self::NON_FUNGIBLE.bits;
        const BURN_NON_FUNGIBLE    = 0b1000_0000 | Self::NON_FUNGIBLE.bits;
        const SIDECHAIN_CHECKPOINT = 0b0001_0000 | Self::NON_FUNGIBLE.bits;
        const COMMITTEE_DEFINITION = 0b0010_0000 | Self::NON_FUNGIBLE.bits;
    }
}

impl ConsensusEncoding for OutputFlags {
    fn consensus_encode<W: io::Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
        writer.write(&self.bits.to_le_bytes())
    }
}

impl ConsensusEncodingSized for OutputFlags {
    fn consensus_encode_exact_size(&self) -> usize {
        1
    }
}

impl ConsensusDecoding for OutputFlags {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let mut buf = [0u8; 1];
        reader.read_exact(&mut buf)?;
        // SAFETY: we have 3 options here:
        // 1. error if unsupported flags are used, meaning that every new flag will be a hard fork
        // 2. truncate unsupported flags, means different hashes will be produced for the same block
        // 3. ignore unsupported flags, which could be set at any time and persisted to the blockchain.
        //   Once those flags are defined at some point in the future, depending on the functionality of the flag,
        //   a consensus rule may be needed that ignores flags prior to a given block height.
        // Option 3 is used here
        Ok(unsafe { OutputFlags::from_bits_unchecked(u8::from_le_bytes(buf)) })
    }
}
