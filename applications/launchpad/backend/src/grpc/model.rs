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

use log::info;
use serde::Serialize;
use tari_app_grpc::tari_rpc::{
    GetBalanceResponse,
    GetIdentityResponse,
    NodeIdentity,
    SyncProgressResponse,
    TransactionEvent,
    TransferResponse,
};
use tari_common_types::{emoji::EmojiId, types::PublicKey};

pub const BLOCKS_SYNC_EXPECTED_TIME_SEC: u64 = 7200;
pub const HEADERS_SYNC_EXPECTED_TIME_SEC: u64 = 1800;
pub const HEADER: i32 = 2;
pub const BLOCK: i32 = 4;

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

#[derive(Debug, Clone, Serialize)]
pub struct TransferFunds {
    pub payments: Vec<Payment>,
}

#[derive(Debug, Clone, Serialize)]
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

#[derive(Serialize, Clone, Debug)]
pub struct SyncProgressInfo {
    pub sync_type: SyncType,
    pub starting_items_index: u64,
    pub synced_items: u64,
    pub total_items: u64,
    pub elapsed_time_sec: u64,
    pub min_estimated_time_sec: u64,
    pub max_estimated_time_sec: u64,
}

#[derive(Debug, Clone)]
pub struct SyncProgress {
    pub sync_type: SyncType,
    pub start_time: Instant,
    pub started: bool,
    pub start_index: u64,
    pub total_items: u64,
    pub sync_items: u64,
    pub new_items: u64,
    pub min_remaining_time: u64,
    pub max_remaining_time: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum SyncType {
    Block,
    Header,
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
            sync_type: if value.state == HEADER as i32 {
                Some(SyncType::Header)
            } else if value.state == BLOCK as i32 {
                Some(SyncType::Block)
            } else {
                None
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

impl SyncProgress {
    pub fn new(sync_type: SyncType, local_height: u64, tip_height: u64) -> Self {
        SyncProgress {
            sync_type,
            started: false,
            start_index: local_height,
            total_items: tip_height,
            start_time: Instant::now(),
            sync_items: 0,
            max_remaining_time: 7200,
            min_remaining_time: 0,
            new_items: 0,
        }
    }

    pub fn sync_local_items(&mut self, local_height: u64) {
        self.sync_items = local_height;
    }

    pub fn sync_total_items(&mut self, tip_height: u64) {
        self.new_items = tip_height - self.total_items;
    }

    /// Init and start progress tracking blocks syncing.
    pub fn start(&mut self, local_height: u64, tip_height: u64) {
        self.start_index = local_height;
        self.sync_items = local_height;
        self.total_items = tip_height;
        self.start_time = Instant::now();
        self.started = true;
    }

    /// Update sync_items and cacludate remaing times.
    pub fn sync(&mut self, local_height: u64, tip_height: u64) {
        self.sync_items = local_height;
        self.total_items = tip_height;
        self.calucate_estimated_times();
    }

    /// Calculates max_remaining_time and min_remaining_time based on progress rate.
    pub fn calucate_estimated_times(&mut self) {
        let expected_time_in_sec = match self.sync_type {
            SyncType::Block => BLOCKS_SYNC_EXPECTED_TIME_SEC,
            SyncType::Header => HEADERS_SYNC_EXPECTED_TIME_SEC,
        } as f32;
        let elapsed_time_in_sec = self.start_time.elapsed().as_secs_f32();
        let current_progress = self.calculate_progress_rate();
        self.min_remaining_time = (elapsed_time_in_sec * (100.0 - current_progress) / current_progress) as u64;
        let remaining_parts: f32 = (100.0 - current_progress as f32) / 100.0;
        self.max_remaining_time = (expected_time_in_sec * remaining_parts) as u64;
    }

    fn calculate_progress_rate(&self) -> f32 {
        let all_items = (self.total_items - self.start_index) as f32;
        let all_local_items = (self.sync_items - self.start_index) as f32;
        (all_local_items * 100.0) / (all_items)
    }
}

impl SyncProgressInfo {
    fn new(sync_type: SyncType, synced_items: u64, total_items: u64) -> Self {
        SyncProgressInfo {
            sync_type: sync_type.clone(),
            starting_items_index: synced_items,
            synced_items,
            total_items,
            elapsed_time_sec: 0,
            min_estimated_time_sec: 0,
            max_estimated_time_sec: match sync_type {
                SyncType::Header => HEADERS_SYNC_EXPECTED_TIME_SEC,
                _ => BLOCKS_SYNC_EXPECTED_TIME_SEC,
            },
        }
    }
}

impl From<SyncProgress> for SyncProgressInfo {
    fn from(source: SyncProgress) -> Self {
        SyncProgressInfo {
            sync_type: source.sync_type,
            starting_items_index: source.start_index,
            synced_items: source.sync_items,
            total_items: source.total_items,
            elapsed_time_sec: source.start_time.elapsed().as_secs(),
            max_estimated_time_sec: source.max_remaining_time,
            min_estimated_time_sec: source.min_remaining_time,
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
