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

use std::{
    convert::TryFrom,
    fmt::{Display, Error, Formatter},
};

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use tari_common_types::{
    transaction::{TransactionConversionError, TransactionDirection, TransactionStatus, TxId},
    types::{BlockHash, PrivateKey, Signature},
};
use tari_comms::types::CommsPublicKey;
use tari_core::transactions::{
    tari_amount::MicroTari,
    transaction_components::Transaction,
    ReceiverTransactionProtocol,
    SenderTransactionProtocol,
};
use tari_utilities::hex::Hex;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InboundTransaction {
    pub tx_id: TxId,
    pub source_public_key: CommsPublicKey,
    pub amount: MicroTari,
    pub receiver_protocol: ReceiverTransactionProtocol,
    pub status: TransactionStatus,
    pub message: String,
    pub timestamp: NaiveDateTime,
    pub cancelled: bool,
    pub direct_send_success: bool,
    pub send_count: u32,
    pub last_send_timestamp: Option<NaiveDateTime>,
}

impl InboundTransaction {
    pub fn new(
        tx_id: TxId,
        source_public_key: CommsPublicKey,
        amount: MicroTari,
        receiver_protocol: ReceiverTransactionProtocol,
        status: TransactionStatus,
        message: String,
        timestamp: NaiveDateTime,
    ) -> Self {
        Self {
            tx_id,
            source_public_key,
            amount,
            receiver_protocol,
            status,
            message,
            timestamp,
            cancelled: false,
            direct_send_success: false,
            send_count: 0,
            last_send_timestamp: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OutboundTransaction {
    pub tx_id: TxId,
    pub destination_public_key: CommsPublicKey,
    pub amount: MicroTari,
    pub fee: MicroTari,
    pub sender_protocol: SenderTransactionProtocol,
    pub status: TransactionStatus,
    pub message: String,
    pub timestamp: NaiveDateTime,
    pub cancelled: bool,
    pub direct_send_success: bool,
    pub send_count: u32,
    pub last_send_timestamp: Option<NaiveDateTime>,
}

impl OutboundTransaction {
    pub fn new(
        tx_id: TxId,
        destination_public_key: CommsPublicKey,
        amount: MicroTari,
        fee: MicroTari,
        sender_protocol: SenderTransactionProtocol,
        status: TransactionStatus,
        message: String,
        timestamp: NaiveDateTime,
        direct_send_success: bool,
    ) -> Self {
        Self {
            tx_id,
            destination_public_key,
            amount,
            fee,
            sender_protocol,
            status,
            message,
            timestamp,
            cancelled: false,
            direct_send_success,
            send_count: 0,
            last_send_timestamp: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompletedTransaction {
    pub tx_id: TxId,
    pub source_public_key: CommsPublicKey,
    pub destination_public_key: CommsPublicKey,
    pub amount: MicroTari,
    pub fee: MicroTari,
    pub transaction: Transaction,
    pub status: TransactionStatus,
    pub message: String,
    pub timestamp: NaiveDateTime,
    pub cancelled: Option<TxCancellationReason>,
    pub direction: TransactionDirection,
    pub coinbase_block_height: Option<u64>,
    pub send_count: u32,
    pub last_send_timestamp: Option<NaiveDateTime>,
    pub transaction_signature: Signature,
    pub confirmations: Option<u64>,
    pub mined_height: Option<u64>,
    pub mined_in_block: Option<BlockHash>,
}

impl CompletedTransaction {
    pub fn new(
        tx_id: TxId,
        source_public_key: CommsPublicKey,
        destination_public_key: CommsPublicKey,
        amount: MicroTari,
        fee: MicroTari,
        transaction: Transaction,
        status: TransactionStatus,
        message: String,
        timestamp: NaiveDateTime,
        direction: TransactionDirection,
        coinbase_block_height: Option<u64>,
        mined_height: Option<u64>,
    ) -> Self {
        let transaction_signature = if let Some(excess_sig) = transaction.first_kernel_excess_sig() {
            excess_sig.clone()
        } else {
            Signature::default()
        };
        Self {
            tx_id,
            source_public_key,
            destination_public_key,
            amount,
            fee,
            transaction,
            status,
            message,
            timestamp,
            cancelled: None,
            direction,
            coinbase_block_height,
            send_count: 0,
            last_send_timestamp: None,
            transaction_signature,
            confirmations: None,
            mined_height,
            mined_in_block: None,
        }
    }

    pub fn get_unique_id(&self) -> Option<String> {
        let body = self.transaction.body();
        for tx_input in body.inputs() {
            if let Ok(features) = tx_input.features() {
                if let Some(ref unique_id) = features.unique_id {
                    return Some(unique_id.to_hex());
                }
            }
        }
        for tx_output in body.outputs() {
            if let Some(ref unique_id) = tx_output.features.unique_id {
                return Some(unique_id.to_hex());
            }
        }
        None
    }

    pub fn is_coinbase(&self) -> bool {
        if let Some(height) = self.coinbase_block_height {
            height > 0
        } else {
            false
        }
    }
}

impl From<CompletedTransaction> for InboundTransaction {
    fn from(ct: CompletedTransaction) -> Self {
        Self {
            tx_id: ct.tx_id,
            source_public_key: ct.source_public_key,
            amount: ct.amount,
            receiver_protocol: ReceiverTransactionProtocol::new_placeholder(),
            status: ct.status,
            message: ct.message,
            timestamp: ct.timestamp,
            cancelled: ct.cancelled.is_some(),
            direct_send_success: false,
            send_count: 0,
            last_send_timestamp: None,
        }
    }
}

impl From<CompletedTransaction> for OutboundTransaction {
    fn from(ct: CompletedTransaction) -> Self {
        Self {
            tx_id: ct.tx_id,
            destination_public_key: ct.destination_public_key,
            amount: ct.amount,
            fee: ct.fee,
            sender_protocol: SenderTransactionProtocol::new_placeholder(),
            status: ct.status,
            message: ct.message,
            timestamp: ct.timestamp,
            cancelled: ct.cancelled.is_some(),
            direct_send_success: false,
            send_count: 0,
            last_send_timestamp: None,
        }
    }
}

impl From<OutboundTransaction> for CompletedTransaction {
    fn from(tx: OutboundTransaction) -> Self {
        let transaction = if tx.sender_protocol.is_finalized() {
            match tx.sender_protocol.get_transaction() {
                Ok(tx) => tx.clone(),
                Err(_) => Transaction::new(vec![], vec![], vec![], PrivateKey::default(), PrivateKey::default()),
            }
        } else {
            Transaction::new(vec![], vec![], vec![], PrivateKey::default(), PrivateKey::default())
        };
        let transaction_signature = if let Some(excess_sig) = transaction.first_kernel_excess_sig() {
            excess_sig.clone()
        } else {
            Signature::default()
        };
        Self {
            tx_id: tx.tx_id,
            source_public_key: Default::default(),
            destination_public_key: tx.destination_public_key,
            amount: tx.amount,
            fee: tx.fee,
            status: tx.status,
            message: tx.message,
            timestamp: tx.timestamp,
            cancelled: if tx.cancelled {
                Some(TxCancellationReason::UserCancelled)
            } else {
                None
            },
            transaction,
            direction: TransactionDirection::Outbound,
            coinbase_block_height: None,
            send_count: 0,
            last_send_timestamp: None,
            transaction_signature,
            confirmations: None,
            mined_height: None,
            mined_in_block: None,
        }
    }
}

impl From<InboundTransaction> for CompletedTransaction {
    fn from(tx: InboundTransaction) -> Self {
        Self {
            tx_id: tx.tx_id,
            source_public_key: tx.source_public_key,
            destination_public_key: Default::default(),
            amount: tx.amount,
            fee: MicroTari::from(0),
            status: tx.status,
            message: tx.message,
            timestamp: tx.timestamp,
            cancelled: if tx.cancelled {
                Some(TxCancellationReason::UserCancelled)
            } else {
                None
            },
            transaction: Transaction::new(vec![], vec![], vec![], PrivateKey::default(), PrivateKey::default()),
            direction: TransactionDirection::Inbound,
            coinbase_block_height: None,
            send_count: 0,
            last_send_timestamp: None,
            transaction_signature: Signature::default(),
            confirmations: None,
            mined_height: None,
            mined_in_block: None,
        }
    }
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum WalletTransaction {
    PendingInbound(InboundTransaction),
    PendingOutbound(OutboundTransaction),
    Completed(CompletedTransaction),
}

impl From<WalletTransaction> for CompletedTransaction {
    fn from(tx: WalletTransaction) -> Self {
        match tx {
            WalletTransaction::PendingInbound(tx) => CompletedTransaction::from(tx),
            WalletTransaction::PendingOutbound(tx) => CompletedTransaction::from(tx),
            WalletTransaction::Completed(tx) => tx,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TxCancellationReason {
    Unknown,            // 0
    UserCancelled,      // 1
    Timeout,            // 2
    DoubleSpend,        // 3
    Orphan,             // 4
    TimeLocked,         // 5
    InvalidTransaction, // 6
    AbandonedCoinbase,  // 7
}

impl TryFrom<u32> for TxCancellationReason {
    type Error = TransactionConversionError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(TxCancellationReason::Unknown),
            1 => Ok(TxCancellationReason::UserCancelled),
            2 => Ok(TxCancellationReason::Timeout),
            3 => Ok(TxCancellationReason::DoubleSpend),
            4 => Ok(TxCancellationReason::Orphan),
            5 => Ok(TxCancellationReason::TimeLocked),
            6 => Ok(TxCancellationReason::InvalidTransaction),
            7 => Ok(TxCancellationReason::AbandonedCoinbase),
            code => Err(TransactionConversionError { code: code as i32 }),
        }
    }
}

impl Display for TxCancellationReason {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        use TxCancellationReason::*;
        let response = match self {
            Unknown => "Unknown",
            UserCancelled => "User Cancelled",
            Timeout => "Timeout",
            DoubleSpend => "Double Spend",
            Orphan => "Orphan",
            TimeLocked => "TimeLocked",
            InvalidTransaction => "Invalid Transaction",
            AbandonedCoinbase => "Abandoned Coinbase",
        };
        fmt.write_str(response)
    }
}
