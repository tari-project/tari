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

use tari_common_types::types::Commitment;

#[derive(Debug, Copy, Clone, Default, Eq, PartialEq)]
pub enum UtxoSelectionMode {
    #[default]
    Safe,
    ListingOnly,
}

#[derive(Debug, Clone, Default)]
pub struct UtxoSelectionCriteria {
    pub mode: UtxoSelectionMode,
    pub filter: UtxoSelectionFilter,
    pub ordering: UtxoSelectionOrdering,
    pub excluding: Vec<Commitment>,
    pub excluding_onesided: bool,
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

    pub fn specific(commitments: Vec<Commitment>) -> Self {
        Self {
            filter: UtxoSelectionFilter::SpecificOutputs { commitments },
            ordering: UtxoSelectionOrdering::Default,
            ..Default::default()
        }
    }
}

impl Display for UtxoSelectionCriteria {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "filter: {}, ordering: {}", self.filter, self.ordering)
    }
}

/// UTXO selection ordering
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UtxoSelectionOrdering {
    /// The Default ordering is heuristic and depends on the requested value and the value of the available UTXOs.
    /// If the requested value is larger than the largest available UTXO, we select LargerFirst as inputs, otherwise
    /// SmallestFirst.
    #[default]
    Default,
    /// Start from the smallest UTXOs and work your way up until the amount is covered. Main benefit
    /// is removing small UTXOs from the blockchain, con is that it costs more in fees
    SmallestFirst,
    /// A strategy that selects the largest UTXOs first. Preferred when the amount is large
    LargestFirst,
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

#[derive(Debug, Clone, Default)]
pub enum UtxoSelectionFilter {
    /// Select OutputType::Standard or OutputType::Coinbase outputs only
    #[default]
    Standard,
    /// Selects specific outputs. All outputs must be exist and be spendable.
    SpecificOutputs { commitments: Vec<Commitment> },
}
impl UtxoSelectionFilter {
    pub fn is_standard(&self) -> bool {
        matches!(self, UtxoSelectionFilter::Standard)
    }
}

impl Display for UtxoSelectionFilter {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            UtxoSelectionFilter::Standard => {
                write!(f, "Standard")
            },
            UtxoSelectionFilter::SpecificOutputs { commitments: outputs } => {
                write!(f, "Specific({} output(s))", outputs.len())
            },
        }
    }
}
