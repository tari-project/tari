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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::block::AggregateBody;
use derive_error::Error;
use serde::{Deserialize, Serialize};
use tari_infra_derive::Hashable;
use tari_utilities::{ExtendBytes, Hashable};

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct MockProofOfWork {
    work: u64,
}
/// This describes the interface the block validation will use when interacting with the proof of work.
pub trait ProofOfWorkInterface {
    /// This function will compare another proof of work. It will return true if the other is higher.
    fn has_more_accum_work_than(&self, other: &Self) -> bool;
    /// This function provides the proof that is supplied in the block header as bytes.
    fn proof_as_bytes(&self) -> Vec<u8>;
    /// This function  will validate the proof of work for the given block.
    fn validate_pow(&self, body: &AggregateBody) -> Result<(), PoWError>;
}

#[derive(Clone, Debug, PartialEq, Error)]
pub enum PoWError {
    // ProofOfWorkFailed
    InvalidProofOfWork,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct MockProofOfWork {
    pub work: u64,
}

impl MockProofOfWork {
    pub fn new() -> MockProofOfWork {
        MockProofOfWork { work: 0 }
    }
}

impl ProofOfWorkInterface for MockProofOfWork {
    fn proof_as_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        bincode::serialize_into(&mut buf, self).unwrap(); // this should not fail
        buf
    }

    fn has_more_accum_work_than(&self, other: &MockProofOfWork) -> bool {
        self.work < other.work
    }

    fn validate_pow(&self, _body: &AggregateBody) -> Result<(), PoWError> {
        Ok(())
    }
}
