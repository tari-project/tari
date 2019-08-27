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

use crate::{mempool::unconfirmed_pool::UnconfirmedPoolError, transaction::Transaction};
use std::{convert::TryFrom, sync::Arc};
use tari_utilities::message_format::MessageFormat;

/// Create a unique unspent transaction priority based on the transaction fee, age of the oldest input UTXO and the
/// excess_sig. The excess_sig is included to ensure the the priority key unique so it can be used with a BTreeMap.
/// Normally, duplicate keys will be overwritten in a BTreeMap.
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct TxPriority(Vec<u8>);

impl TxPriority {
    pub fn try_from(transaction: &Transaction) -> Result<Self, UnconfirmedPoolError> {
        let fee_per_byte = (transaction.calculate_ave_fee_per_gram() * 1000.0) as usize; // Include 3 decimal places before flooring
        let mut priority = fee_per_byte.to_binary()?;
        priority.reverse(); // Fee needs to be in Big-endian for sorting with BtreeMap to work correctly
                            // TODO: Add oldest input UTXO age
        priority.append(&mut transaction.body.kernels[0].to_binary()?);
        Ok(Self(priority))
    }
}

impl Clone for TxPriority {
    fn clone(&self) -> Self {
        TxPriority(self.0.clone())
    }
}

/// A prioritized transaction includes a transaction and the calculated priority of the transaction.
pub struct PrioritizedTransaction {
    pub transaction: Arc<Transaction>,
    pub priority: TxPriority,
    pub weight: u64,
}

impl TryFrom<Transaction> for PrioritizedTransaction {
    type Error = UnconfirmedPoolError;

    fn try_from(transaction: Transaction) -> Result<Self, Self::Error> {
        Ok(Self {
            priority: TxPriority::try_from(&transaction)?,
            weight: transaction.calculate_weight(),
            transaction: Arc::new(transaction),
        })
    }
}
