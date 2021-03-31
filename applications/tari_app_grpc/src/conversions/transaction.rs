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

use crate::tari_rpc as grpc;
use std::convert::{TryFrom, TryInto};
use tari_core::transactions::transaction::Transaction;
use tari_crypto::{ristretto::RistrettoSecretKey, tari_utilities::ByteArray};
use tari_wallet::{output_manager_service::TxId, transaction_service::storage::models};

impl From<Transaction> for grpc::Transaction {
    fn from(source: Transaction) -> Self {
        Self {
            offset: Vec::from(source.offset.as_bytes()),
            body: Some(source.body.into()),
        }
    }
}

impl TryFrom<grpc::Transaction> for Transaction {
    type Error = String;

    fn try_from(source: grpc::Transaction) -> Result<Self, Self::Error> {
        Ok(Self {
            offset: RistrettoSecretKey::from_bytes(&source.offset)
                .map_err(|e| format!("Offset is not valid:{}", e.to_string()))?,
            body: source
                .body
                .ok_or_else(|| "Transaction body not provided".to_string())?
                .try_into()?,
        })
    }
}

impl From<models::TransactionStatus> for grpc::TransactionStatus {
    fn from(status: models::TransactionStatus) -> Self {
        use models::TransactionStatus::*;
        match status {
            Completed => grpc::TransactionStatus::Completed,
            Broadcast => grpc::TransactionStatus::Broadcast,
            MinedUnconfirmed => grpc::TransactionStatus::MinedUnconfirmed,
            MinedConfirmed => grpc::TransactionStatus::MinedConfirmed,
            Imported => grpc::TransactionStatus::Imported,
            Pending => grpc::TransactionStatus::Pending,
            Coinbase => grpc::TransactionStatus::Coinbase,
        }
    }
}

impl From<models::TransactionDirection> for grpc::TransactionDirection {
    fn from(status: models::TransactionDirection) -> Self {
        use models::TransactionDirection::*;
        match status {
            Unknown => grpc::TransactionDirection::Unknown,
            Inbound => grpc::TransactionDirection::Inbound,
            Outbound => grpc::TransactionDirection::Outbound,
        }
    }
}

impl grpc::TransactionInfo {
    pub fn not_found(tx_id: TxId) -> Self {
        Self {
            tx_id,
            is_found: false,
            ..Default::default()
        }
    }
}
