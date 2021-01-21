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

use crate::{output_manager_service::TxId, transaction_service::error::TransactionStorageError};
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::{
    convert::TryFrom,
    fmt::{Display, Error, Formatter},
};
use tari_comms::types::CommsPublicKey;
use tari_core::transactions::{
    tari_amount::MicroTari,
    transaction::Transaction,
    types::PrivateKey,
    ReceiverTransactionProtocol,
    SenderTransactionProtocol,
};
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransactionStatus {
    /// This transaction has been completed between the parties but has not been broadcast to the base layer network.
    Completed,
    /// This transaction has been broadcast to the base layer network and is currently in one or more base node
    /// mempools.
    /// TODO This status is no longer used but it is left here for backward compatibility. A transaction will be
    /// Completed and transition straight to mined
    Broadcast,
    /// This transaction has been mined and included in a block.
    Mined,
    /// This transaction was generated as part of importing a spendable UTXO
    Imported,
    /// This transaction is still being negotiated by the parties
    Pending,
    /// This is a created Coinbase Transaction
    Coinbase,
}

impl TryFrom<i32> for TransactionStatus {
    type Error = TransactionStorageError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(TransactionStatus::Completed),
            1 => Ok(TransactionStatus::Broadcast),
            2 => Ok(TransactionStatus::Mined),
            3 => Ok(TransactionStatus::Imported),
            4 => Ok(TransactionStatus::Pending),
            5 => Ok(TransactionStatus::Coinbase),
            _ => Err(TransactionStorageError::ConversionError(
                "Invalid TransactionStatus".to_string(),
            )),
        }
    }
}

impl Default for TransactionStatus {
    fn default() -> Self {
        TransactionStatus::Pending
    }
}

impl Display for TransactionStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        // No struct or tuple variants
        match self {
            TransactionStatus::Completed => write!(f, "Completed"),
            TransactionStatus::Broadcast => write!(f, "Broadcast"),
            TransactionStatus::Mined => write!(f, "Mined"),
            TransactionStatus::Imported => write!(f, "Imported"),
            TransactionStatus::Pending => write!(f, "Pending"),
            TransactionStatus::Coinbase => write!(f, "Coinbase"),
        }
    }
}

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
    ) -> Self
    {
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
    #[allow(clippy::too_many_arguments)]
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
    ) -> Self
    {
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
    pub cancelled: bool,
    pub direction: TransactionDirection,
    pub coinbase_block_height: Option<u64>,
    pub send_count: u32,
    pub last_send_timestamp: Option<NaiveDateTime>,
}

impl CompletedTransaction {
    #[allow(clippy::too_many_arguments)]
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
    ) -> Self
    {
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
            cancelled: false,
            direction,
            coinbase_block_height,
            send_count: 0,
            last_send_timestamp: None,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TransactionDirection {
    Inbound,
    Outbound,
    Unknown,
}

impl TryFrom<i32> for TransactionDirection {
    type Error = TransactionStorageError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(TransactionDirection::Inbound),
            1 => Ok(TransactionDirection::Outbound),
            2 => Ok(TransactionDirection::Unknown),
            _ => Err(TransactionStorageError::ConversionError(
                "Invalid TransactionDirection".to_string(),
            )),
        }
    }
}

impl Display for TransactionDirection {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        // No struct or tuple variants
        match self {
            TransactionDirection::Inbound => write!(f, "Inbound"),
            TransactionDirection::Outbound => write!(f, "Outbound"),
            TransactionDirection::Unknown => write!(f, "Unknown"),
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
            cancelled: ct.cancelled,
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
            cancelled: ct.cancelled,
            direct_send_success: false,
            send_count: 0,
            last_send_timestamp: None,
        }
    }
}

impl From<OutboundTransaction> for CompletedTransaction {
    fn from(tx: OutboundTransaction) -> Self {
        Self {
            tx_id: tx.tx_id,
            source_public_key: Default::default(),
            destination_public_key: tx.destination_public_key,
            amount: tx.amount,
            fee: tx.fee,
            status: tx.status,
            message: tx.message,
            timestamp: tx.timestamp,
            cancelled: tx.cancelled,
            transaction: Transaction::new(vec![], vec![], vec![], PrivateKey::default()),
            direction: TransactionDirection::Outbound,
            coinbase_block_height: None,
            send_count: 0,
            last_send_timestamp: None,
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
            cancelled: tx.cancelled,
            transaction: Transaction::new(vec![], vec![], vec![], PrivateKey::default()),
            direction: TransactionDirection::Inbound,
            coinbase_block_height: None,
            send_count: 0,
            last_send_timestamp: None,
        }
    }
}

#[derive(Debug)]
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
