// Copyright 2020. The Tari Project
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

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputManagerServiceConfig {
    /// If a large amount of tiny valued uT UTXOs are used as inputs to a transaction, the fee may be larger than the
    /// transaction amount. Set this value to `false` to allow spending of "dust" UTXOs for small valued transactions.
    pub prevent_fee_gt_amount: bool,
    /// Ignores dust below this value, value in micro MinoTari
    pub dust_ignore_value: u64,
    /// This is the size of the event channel used to communicate output manager events to the wallet.
    pub event_channel_size: usize,
    /// The number of confirmations (difference between tip height and mined height) required for the output to be
    /// marked as mined confirmed
    pub num_confirmations_required: u64,
    /// The number of batches the unconfirmed outputs will be divided into before being queried from the base node
    pub tx_validator_batch_size: usize,
    /// Wallets currently will choose the best outputs as inputs when spending, however since a lurking base node can
    /// generate a transaction graph of inputs to outputs with relative ease, a wallet may reveal its transaction
    /// history by including a (non-stealth address) one-sided payment.
    /// If set to `true`, then outputs received via simple one-sided transactions, won't be automatically selected as
    /// inputs for further transactions, but can still be selected individually as specific outputs.
    pub autoignore_onesided_utxos: bool,
    /// The number of seconds that have to pass for the wallet to run revalidation of invalid UTXOs on startup.
    pub num_of_seconds_to_revalidate_invalid_utxos: u64,
}

impl Default for OutputManagerServiceConfig {
    fn default() -> Self {
        Self {
            prevent_fee_gt_amount: true,
            dust_ignore_value: 100,
            event_channel_size: 250,
            num_confirmations_required: 3,
            tx_validator_batch_size: 100,
            autoignore_onesided_utxos: false,
            num_of_seconds_to_revalidate_invalid_utxos: 60 * 60 * 24 * 3,
        }
    }
}
