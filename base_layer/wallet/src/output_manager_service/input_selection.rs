//  Copyright 2022. The Tari Project
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
    fmt,
    fmt::{Display, Formatter},
};

use tari_common_types::types::{Commitment, FixedHash, PublicKey};
use tari_core::transactions::transaction_components::OutputType;

use crate::output_manager_service::storage::models::DbUnblindedOutput;

#[derive(Debug, Clone, Default)]
pub struct UtxoSelectionCriteria {
    pub filter: UtxoSelectionFilter,
    pub ordering: UtxoSelectionOrdering,
    pub excluding: Vec<Commitment>,
}

impl UtxoSelectionCriteria {
    pub fn smallest_first() -> Self {
        Self {
            filter: UtxoSelectionFilter::Standard,
            ordering: UtxoSelectionOrdering::SmallestFirst,
            ..Default::default()
        }
    }

    pub fn largest_first() -> Self {
        Self {
            filter: UtxoSelectionFilter::Standard,
            ordering: UtxoSelectionOrdering::LargestFirst,
            ..Default::default()
        }
    }

    pub fn smallest_first() -> Self {
        Self {
            filter: UtxoSelectionFilter::Standard,
            ordering: UtxoSelectionOrdering::SmallestFirst,
        }
    }

    pub fn for_token(unique_id: Vec<u8>, parent_public_key: Option<PublicKey>) -> Self {
        Self {
            filter: UtxoSelectionFilter::TokenOutput {
                unique_id,
                parent_public_key,
            },
            ..Default::default()
        }
    }

    pub fn for_contract(contract_id: FixedHash, output_type: OutputType) -> Self {
        Self {
            filter: UtxoSelectionFilter::ContractOutput {
                contract_id,
                output_type,
            },
            ..Default::default()
        }
    }

    pub fn specific(commitments: Vec<Commitment>) -> Self {
        Self {
            filter: UtxoSelectionFilter::SpecificOutputs { commitments },
            ordering: UtxoSelectionOrdering::Default,
        }
    }
}

impl Display for UtxoSelectionCriteria {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "filter: {}, ordering: {}", self.filter, self.ordering)
    }
}

/// UTXO selection ordering
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UtxoSelectionOrdering {
    /// The Default ordering is heuristic and depends on the requested value and the value of the available UTXOs.
    /// If the requested value is larger than the largest available UTXO, we select LargerFirst as inputs, otherwise
    /// SmallestFirst.
    Default,
    /// Start from the smallest UTXOs and work your way up until the amount is covered. Main benefit
    /// is removing small UTXOs from the blockchain, con is that it costs more in fees
    SmallestFirst,
    /// A strategy that selects the largest UTXOs first. Preferred when the amount is large
    LargestFirst,
}

impl Default for UtxoSelectionOrdering {
    fn default() -> Self {
        UtxoSelectionOrdering::Default
    }
}

impl Display for UtxoSelectionOrdering {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UtxoSelectionOrdering::SmallestFirst => write!(f, "Smallest"),
            UtxoSelectionOrdering::LargestFirst => write!(f, "Largest"),
            UtxoSelectionOrdering::Default => write!(f, "Default"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum UtxoSelectionFilter {
    /// Select OutputType::Standard or OutputType::Coinbase outputs only
    Standard,
    /// Select matching token outputs. This will be deprecated in future.
    TokenOutput {
        unique_id: Vec<u8>,
        parent_public_key: Option<PublicKey>,
    },
    /// Select matching contract outputs. Additional Standard outputs may be included if necessary.
    ContractOutput {
        /// Contract ID to select
        contract_id: FixedHash,
        /// Type of contract output to select.
        output_type: OutputType,
    },
    /// Selects specific outputs. All outputs must be exist and be spendable.
    SpecificOutputs { commitments: Vec<Commitment> },
}
impl UtxoSelectionFilter {
    pub fn is_standard(&self) -> bool {
        matches!(self, UtxoSelectionFilter::Standard)
    }

    pub fn is_contract_output(&self) -> bool {
        matches!(self, UtxoSelectionFilter::ContractOutput { .. })
    }
}

impl Default for UtxoSelectionFilter {
    fn default() -> Self {
        UtxoSelectionFilter::Standard
    }
}

impl Display for UtxoSelectionFilter {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            UtxoSelectionFilter::Standard => {
                write!(f, "Standard")
            },
            UtxoSelectionFilter::TokenOutput { .. } => {
                write!(f, "TokenOutput{{..}}")
            },
            UtxoSelectionFilter::SpecificOutputs { commitments: outputs } => {
                write!(f, "Specific({} output(s))", outputs.len())
            },
            UtxoSelectionFilter::ContractOutput { contract_id, .. } => {
                write!(f, "ContractOutput({})", contract_id)
            },
        }
    }
}
