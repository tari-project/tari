//  Copyright 2021. The Tari Project
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

use std::{convert::TryFrom, time::Instant};

use derivative::Derivative;
use log::info;
use serde::{Deserialize, Serialize};
use tari_app_grpc::tari_rpc::{
    GetBalanceResponse,
    GetIdentityResponse,
    NodeIdentity,
    SyncProgressResponse,
    TransactionEvent,
    TransferResponse,
};
use tari_common_types::{emoji::EmojiId, types::PublicKey};

pub const HEADER: i32 = 2;
pub const BLOCK: i32 = 4;
pub const DONE: i32 = 5;

pub const STANDARD_MIMBLEWIMBLE: i32 = 0;
pub const ONE_SIDED: i32 = 1;

#[derive(Debug, Clone, Serialize)]
pub struct WalletTransaction {
    pub event: String,
    pub tx_id: String,
    pub source_pk: Vec<u8>,
    pub dest_pk: Vec<u8>,
    pub status: String,
    pub direction: String,
    pub amount: u64,
    pub message: String,
    pub is_coinbase: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletIdentity {
    public_key: Vec<u8>,
    public_address: String,
    node_id: Vec<u8>,
    emoji_id: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct WalletBalance {
    available_balance: u64,
    pending_incoming_balance: u64,
    pending_outgoing_balance: u64,
}

#[derive(Debug, Derivative, Deserialize, Clone, Serialize)]
pub struct TransferFunds {
    pub payments: Vec<Payment>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct Payment {
    pub address: String,
    pub amount: u64,
    pub fee_per_gram: u64,
    pub message: String,
    pub payment_type: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct PaymentResult {
    address: String,
    transaction_id: u64,
    is_success: bool,
    failure_message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransferFundsResult {
    payments: Vec<PaymentResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BlockStateInfo {
    pub tip_height: u64,
    pub local_height: u64,
    pub sync_type: Option<SyncType>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum SyncType {
    Startup,
    Block,
    Header,
    Done,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseNodeIdentity {
    public_key: Vec<u8>,
    public_address: String,
    node_id: Vec<u8>,
    emoji_id: String,
}

impl TryFrom<TransactionEvent> for WalletTransaction {
    type Error = String;

    fn try_from(value: TransactionEvent) -> Result<Self, Self::Error> {
        match value.event.as_str() {
            "not_supported" => Err("event is not supported.".to_string()),
            _ => Ok(WalletTransaction {
                event: value.event,
                tx_id: value.tx_id,
                source_pk: value.source_pk,
                dest_pk: value.dest_pk,
                status: value.status,
                direction: value.direction,
                amount: value.amount,
                message: value.message,
                is_coinbase: value.is_coinbase,
            }),
        }
    }
}

impl From<SyncProgressResponse> for BlockStateInfo {
    fn from(value: SyncProgressResponse) -> Self {
        BlockStateInfo {
            tip_height: value.tip_height,
            local_height: value.local_height,
            sync_type: match value.state as i32 {
                HEADER => Some(SyncType::Header),
                BLOCK => Some(SyncType::Block),
                DONE => Some(SyncType::Done),
                _ => None,
            },
        }
    }
}

impl TryFrom<GetIdentityResponse> for WalletIdentity {
    type Error = String;

    fn try_from(value: GetIdentityResponse) -> Result<Self, Self::Error> {
        let hex_public_key = String::from_utf8(value.public_key.clone()).unwrap();
        let emoji_id = EmojiId::from_hex(&hex_public_key)
            .map_err(|e| format!("Failed to create an emoji: {}", e))?
            .to_string();
        Ok(WalletIdentity {
            public_key: value.public_key,
            public_address: value.public_address,
            node_id: value.node_id,
            emoji_id,
        })
    }
}

impl TryFrom<NodeIdentity> for BaseNodeIdentity {
    type Error = String;

    fn try_from(value: NodeIdentity) -> Result<Self, Self::Error> {
        let hex_public_key = hex::encode(value.public_key.clone());
        let emoji_id = EmojiId::from_hex(&hex_public_key)
            .map_err(|e| format!("Failed to create an emoji: {}", e))?
            .to_string();
        Ok(BaseNodeIdentity {
            public_key: value.public_key,
            public_address: value.public_address,
            node_id: value.node_id,
            emoji_id,
        })
    }
}

impl From<GetBalanceResponse> for WalletBalance {
    fn from(value: GetBalanceResponse) -> WalletBalance {
        WalletBalance {
            available_balance: value.available_balance,
            pending_incoming_balance: value.pending_incoming_balance,
            pending_outgoing_balance: value.pending_outgoing_balance,
        }
    }
}

impl From<TransferResponse> for TransferFundsResult {
    fn from(source: TransferResponse) -> TransferFundsResult {
        let payments: Vec<PaymentResult> = source
            .results
            .into_iter()
            .map(|p| PaymentResult {
                address: p.address,
                transaction_id: p.transaction_id,
                is_success: p.is_success,
                failure_message: p.failure_message,
            })
            .collect();
        TransferFundsResult { payments }
    }
}
