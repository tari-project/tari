//  Copyright 2019 The Tari Project
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

use crate::{
    mempool::priority::PriorityError,
    transactions::{transaction::Transaction, weight::TransactionWeight},
};
use std::sync::Arc;
use tari_common_types::types::HashOutput;
use tari_crypto::tari_utilities::message_format::MessageFormat;

/// Create a unique unspent transaction priority based on the transaction fee, maturity of the oldest input UTXO and the
/// excess_sig. The excess_sig is included to ensure the the priority key unique so it can be used with a BTreeMap.
/// Normally, duplicate keys will be overwritten in a BTreeMap.
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct FeePriority(Vec<u8>);

impl FeePriority {
    pub fn try_construct(transaction: &Transaction, weight: u64) -> Result<Self, PriorityError> {
        // The weights have been normalised, so the fee priority is now equal to the fee per gram ± a few pct points
        let fee_per_byte = ((transaction.body.get_total_fee().as_u64() as f64 / weight as f64) * 1000.0) as usize; // Include 3 decimal places before flooring
        let mut fee_priority = fee_per_byte.to_binary()?;
        fee_priority.reverse(); // Requires Big-endian for BtreeMap sorting

        let mut maturity_priority = (u64::MAX - transaction.min_input_maturity()).to_binary()?;
        maturity_priority.reverse(); // Requires Big-endian for BtreeMap sorting

        let mut priority = fee_priority;
        priority.append(&mut maturity_priority);
        priority.append(&mut transaction.body.kernels()[0].excess_sig.to_binary()?);
        Ok(Self(priority))
    }
}

impl Clone for FeePriority {
    fn clone(&self) -> Self {
        FeePriority(self.0.clone())
    }
}

/// A prioritized transaction includes a transaction and the calculated priority of the transaction.
#[derive(Clone)]
pub struct PrioritizedTransaction {
    pub transaction: Arc<Transaction>,
    pub priority: FeePriority,
    pub weight: u64,
    pub depended_output_hashes: Vec<HashOutput>,
}

impl PrioritizedTransaction {
    pub fn try_construct(
        weighting: &TransactionWeight,
        transaction: Arc<Transaction>,
        dependent_outputs: Option<Vec<HashOutput>>,
    ) -> Result<PrioritizedTransaction, PriorityError> {
        let depended_output_hashes = match dependent_outputs {
            Some(v) => v,
            None => Vec::new(),
        };
        let weight = transaction.calculate_weight(weighting);
        Ok(Self {
            priority: FeePriority::try_construct(&transaction, weight)?,
            weight,
            transaction,
            depended_output_hashes,
        })
    }
}
