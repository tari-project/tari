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

use serde::{Deserialize, Serialize};
use tari_common_types::types::HashOutput;
use tari_crypto::tari_utilities::Hashable;

use crate::transactions::transaction_components::TransactionOutput;

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PrunedOutput {
    Pruned {
        output_hash: HashOutput,
        witness_hash: HashOutput,
    },
    NotPruned {
        output: TransactionOutput,
    },
}

impl PrunedOutput {
    pub fn is_pruned(&self) -> bool {
        matches!(self, PrunedOutput::Pruned { .. })
    }

    pub fn hash(&self) -> Vec<u8> {
        match self {
            PrunedOutput::Pruned {
                output_hash,
                witness_hash: _,
            } => output_hash.clone(),
            PrunedOutput::NotPruned { output } => output.hash(),
        }
    }

    pub fn as_transaction_output(&self) -> Option<&TransactionOutput> {
        match self {
            PrunedOutput::Pruned { .. } => None,
            PrunedOutput::NotPruned { output } => Some(output),
        }
    }

    pub fn into_unpruned_output(self) -> Option<TransactionOutput> {
        match self {
            PrunedOutput::Pruned { .. } => None,
            PrunedOutput::NotPruned { output } => Some(output),
        }
    }
}
