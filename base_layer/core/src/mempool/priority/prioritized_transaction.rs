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

use std::{
    fmt::{Display, Formatter},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use tari_common_types::types::{HashOutput, PrivateKey, PublicKey};
use tari_utilities::{hex::Hex, ByteArray};

use crate::transactions::{transaction_components::Transaction, weight::TransactionWeight};

/// Create a unique unspent transaction priority based on the transaction fee, maturity of the oldest input UTXO and the
/// excess_sig. The excess_sig is included to ensure the the priority key unique so it can be used with a BTreeMap.
/// Normally, duplicate keys will be overwritten in a BTreeMap.
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
pub struct FeePriority(Vec<u8>);

impl FeePriority {
    pub fn new(transaction: &Transaction, insert_epoch: u64, weight: u64) -> Self {
        // The weights have been normalised, so the fee priority is now equal to the fee per gram ± a few pct points
        // Include 3 decimal places before flooring
        #[allow(clippy::cast_possible_truncation)]
        #[allow(clippy::cast_sign_loss)]
        let fee_per_byte = ((transaction.body.get_total_fee().as_u64() as f64 / weight as f64) * 1000.0) as u64;
        // Big-endian used here, the MSB is in the starting index. The ordering for Vec<u8> is taken from elements left
        // to right and the unconfirmed pool expects the lowest priority to be sorted lowest to highest in the
        // BTreeMap
        let fee_priority = fee_per_byte.to_be_bytes();
        let age_priority = (u64::MAX - insert_epoch).to_be_bytes();

        let mut priority = vec![0u8; 8 + 8 + 64];
        priority[..8].copy_from_slice(&fee_priority[..]);
        priority[8..16].copy_from_slice(&age_priority[..]);
        // Use the aggregate signature and nonce.
        // If a transaction has many kernels, unless they are all identical, the fee priority will be different.
        let (agg_sig, agg_nonce) = transaction
            .body
            .kernels()
            .iter()
            .map(|k| (k.excess_sig.get_signature(), k.excess_sig.get_public_nonce()))
            .fold(
                (PrivateKey::default(), PublicKey::default()),
                |(agg_sk, agg_nonce), (sig, nonce)| (agg_sk + sig, agg_nonce + nonce),
            );
        priority[16..48].copy_from_slice(agg_sig.as_bytes());
        priority[48..80].copy_from_slice(agg_nonce.as_bytes());
        Self(priority)
    }
}

/// A prioritized transaction includes a transaction and the calculated priority of the transaction.
#[derive(Clone)]
pub struct PrioritizedTransaction {
    pub key: usize,
    pub transaction: Arc<Transaction>,
    pub priority: FeePriority,
    pub fee_per_byte: u64,
    pub weight: u64,
    pub dependent_output_hashes: Vec<HashOutput>,
}

impl PrioritizedTransaction {
    pub fn new(
        key: usize,
        weighting: &TransactionWeight,
        transaction: Arc<Transaction>,
        dependent_outputs: Option<Vec<HashOutput>>,
    ) -> std::io::Result<PrioritizedTransaction> {
        let weight = transaction.calculate_weight(weighting)?;
        let insert_epoch = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(n) => n.as_secs(),
            Err(_) => 0,
        };
        Ok(Self {
            key,
            priority: FeePriority::new(&transaction, insert_epoch, weight),
            fee_per_byte: ((transaction.body.get_total_fee() * 1000) / weight).as_u64(),
            weight,
            transaction,
            dependent_output_hashes: dependent_outputs.unwrap_or_default(),
        })
    }
}

impl Display for PrioritizedTransaction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let sig_hex = self
            .transaction
            .first_kernel_excess_sig()
            .map(|sig| sig.get_signature().to_hex())
            .unwrap_or_else(|| "No kernels!".to_string());
        write!(f, "{} (weight: {}, internal key: {})", sig_hex, self.weight, self.key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transactions::{
        tari_amount::{uT, MicroMinotari, T},
        test_helpers::{create_test_core_key_manager_with_memory_db, create_tx, TestKeyManager},
    };

    async fn create_tx_with_fee(fee_per_gram: MicroMinotari, key_manager: &TestKeyManager) -> Transaction {
        let (tx, _, _) = create_tx(10 * T, fee_per_gram, 0, 1, 0, 1, Default::default(), key_manager)
            .await
            .expect("Failed to get tx");
        tx
    }

    #[tokio::test]
    async fn fee_increases_priority() {
        let key_manager = create_test_core_key_manager_with_memory_db();
        let weighting = TransactionWeight::latest();
        let epoch = u64::MAX / 2;
        let tx = create_tx_with_fee(2 * uT, &key_manager).await;
        let p1 = FeePriority::new(&tx, epoch, tx.calculate_weight(&weighting).expect("Failed to get tx"));

        let tx = create_tx_with_fee(3 * uT, &key_manager).await;
        let p2 = FeePriority::new(&tx, epoch, tx.calculate_weight(&weighting).expect("Failed to get tx"));

        assert!(p2 > p1);
    }

    #[tokio::test]
    async fn age_increases_priority() {
        let key_manager = create_test_core_key_manager_with_memory_db();
        let weighting = TransactionWeight::latest();
        let epoch = u64::MAX / 2;
        let tx = create_tx_with_fee(2 * uT, &key_manager).await;
        let p1 = FeePriority::new(&tx, epoch, tx.calculate_weight(&weighting).expect("Failed to get tx"));

        let tx = create_tx_with_fee(2 * uT, &key_manager).await;
        let p2 = FeePriority::new(
            &tx,
            epoch - 1,
            tx.calculate_weight(&weighting).expect("Failed to get tx"),
        );

        assert!(p2 > p1);
    }
}
