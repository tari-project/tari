// Copyright 2019. The Tari Project
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

use std::io::{Error, Write};

use serde::{Deserialize, Serialize};

use crate::consensus::{ConsensusEncoding, ConsensusEncodingSized};

bitflags! {
    /// Options for a kernel's structure or use.
    /// TODO:  expand to accommodate Tari DAN transaction types, such as namespace and validator node registrations
    #[derive(Deserialize, Serialize)]
    pub struct KernelFeatures: u8 {
        /// Coinbase transaction
        const COINBASE_KERNEL = 1u8;
    }
}

impl KernelFeatures {
    pub fn create_coinbase() -> KernelFeatures {
        KernelFeatures::COINBASE_KERNEL
    }
}

impl ConsensusEncoding for KernelFeatures {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<usize, Error> {
        writer.write_all(&[self.bits][..])?;
        Ok(1)
    }
}

impl ConsensusEncodingSized for KernelFeatures {
    fn consensus_encode_exact_size(&self) -> usize {
        1
    }
}
