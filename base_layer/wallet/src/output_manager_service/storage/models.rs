// Copyright 2012. The Tari Project
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

use crate::output_manager_service::error::OutputManagerStorageError;
use std::cmp::Ordering;
use tari_core::{
    tari_utilities::hash::Hashable,
    transactions::{
        transaction::UnblindedOutput,
        transaction_protocol::RewindData,
        types::{Commitment, CryptoFactories, HashOutput},
    },
};

#[derive(Debug, Clone)]
pub struct DbUnblindedOutput {
    pub commitment: Commitment,
    pub unblinded_output: UnblindedOutput,
    pub hash: HashOutput,
}

impl DbUnblindedOutput {
    pub fn from_unblinded_output(
        output: UnblindedOutput,
        factory: &CryptoFactories,
    ) -> Result<DbUnblindedOutput, OutputManagerStorageError>
    {
        let tx_out = output.as_transaction_output(factory)?;
        Ok(DbUnblindedOutput {
            hash: tx_out.hash(),
            commitment: tx_out.commitment,
            unblinded_output: output,
        })
    }

    pub fn rewindable_from_unblinded_output(
        output: UnblindedOutput,
        factory: &CryptoFactories,
        rewind_data: &RewindData,
    ) -> Result<DbUnblindedOutput, OutputManagerStorageError>
    {
        let tx_out = output.as_rewindable_transaction_output(factory, rewind_data)?;
        Ok(DbUnblindedOutput {
            hash: tx_out.hash(),
            commitment: tx_out.commitment,
            unblinded_output: output,
        })
    }
}

impl From<DbUnblindedOutput> for UnblindedOutput {
    fn from(value: DbUnblindedOutput) -> UnblindedOutput {
        value.unblinded_output
    }
}

impl PartialEq for DbUnblindedOutput {
    fn eq(&self, other: &DbUnblindedOutput) -> bool {
        self.unblinded_output.value == other.unblinded_output.value
    }
}

impl PartialOrd<DbUnblindedOutput> for DbUnblindedOutput {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.unblinded_output.value.partial_cmp(&other.unblinded_output.value)
    }
}

impl Ord for DbUnblindedOutput {
    fn cmp(&self, other: &Self) -> Ordering {
        self.unblinded_output.value.cmp(&other.unblinded_output.value)
    }
}

impl Eq for DbUnblindedOutput {}
