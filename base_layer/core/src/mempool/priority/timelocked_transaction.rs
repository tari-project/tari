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
    mempool::priority::{FeePriority, PriorityError},
    transaction::Transaction,
};
use std::{convert::TryFrom, sync::Arc};
use tari_utilities::message_format::MessageFormat;

/// Create a unique transaction priority based on the maximum time-lock (lock_height or input UTXO maturity) and the
/// excess_sig, allowing transactions to be sorted according to their time-lock expiry. The excess_sig is included to
/// ensure the priority key is unique so it can be used with a BTreeMap.
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct TimelockPriority(Vec<u8>);

impl TimelockPriority {
    pub fn try_from(transaction: &Transaction) -> Result<Self, PriorityError> {
        let mut priority = transaction.max_timelock_height().to_binary()?;
        priority.reverse(); // Requires Big-endian for BtreeMap sorting
        priority.append(&mut transaction.body.kernels[0].excess_sig.to_binary()?);
        Ok(Self(priority))
    }
}

impl Clone for TimelockPriority {
    fn clone(&self) -> Self {
        TimelockPriority(self.0.clone())
    }
}

/// A Timelocked prioritized transaction includes a transaction and the calculated FeePriority and TimelockPriority of
/// the transaction.
pub struct TimelockedTransaction {
    pub transaction: Arc<Transaction>,
    pub fee_priority: FeePriority,
    pub timelock_priority: TimelockPriority,
    pub max_timelock_height: u64,
}

impl TryFrom<Transaction> for TimelockedTransaction {
    type Error = PriorityError;

    fn try_from(transaction: Transaction) -> Result<Self, Self::Error> {
        Ok(Self {
            fee_priority: FeePriority::try_from(&transaction)?,
            timelock_priority: TimelockPriority::try_from(&transaction)?,
            max_timelock_height: transaction.max_timelock_height(),
            transaction: Arc::new(transaction),
        })
    }
}
