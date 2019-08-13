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

use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use tari_comms::message::{Message, MessageError};
use tari_core::transaction::Transaction;
use tari_p2p::tari_message::{BlockchainMessage, TariMessageType};

/// The SyncRequestMessage is used to initiate a Mempool sync between two nodes. It stores a set of known unspent
/// transaction hashes, making it possible for the destination node to only return unknown transactions.
#[derive(Serialize, Deserialize)]
pub struct SyncRequestMessage {
    pub known_tx_hashes: Vec<Vec<u8>>,
}

impl TryInto<Message> for SyncRequestMessage {
    type Error = MessageError;

    fn try_into(self) -> Result<Message, Self::Error> {
        Ok((TariMessageType::new(BlockchainMessage::MempoolSyncRequest), self).try_into()?)
    }
}

/// The SyncReplyMessage is used as a response to a SyncRequestMessage and contains a set of unspent transactions.
#[derive(Serialize, Deserialize)]
pub struct SyncReplyMessage {
    pub utxs: Vec<Transaction>,
}

impl TryInto<Message> for SyncReplyMessage {
    type Error = MessageError;

    fn try_into(self) -> Result<Message, Self::Error> {
        Ok((TariMessageType::new(BlockchainMessage::MempoolSync), self).try_into()?)
    }
}

/// The StatsRequestMessage is used to request the Mempool stats of another node
#[derive(Serialize, Deserialize)]
pub struct StatsRequestMessage {}

impl TryInto<Message> for StatsRequestMessage {
    type Error = MessageError;

    fn try_into(self) -> Result<Message, Self::Error> {
        Ok((TariMessageType::new(BlockchainMessage::MempoolStatsRequest), self).try_into()?)
    }
}

/// The StatsReplyMessage is used to provide the stats of the local Mempool to a requesting Node
#[derive(Serialize, Deserialize)]
pub struct StatsReplyMessage {
    pub unconfirmed_txs_count: u64,
    pub orphaned_txs_count: u64,
    pub timelocked_txs_count: u64,
    pub mempool_size: u64, // The current size of the mempool (in transaction weight)
}

impl TryInto<Message> for StatsReplyMessage {
    type Error = MessageError;

    fn try_into(self) -> Result<Message, Self::Error> {
        Ok((TariMessageType::new(BlockchainMessage::MempoolStats), self).try_into()?)
    }
}

/// UTxsMessage is used to submit new unspent transactions to the network
#[derive(Serialize, Deserialize)]
pub struct UTxsMessage {
    pub utxs: Vec<Transaction>,
}

impl TryInto<Message> for UTxsMessage {
    type Error = MessageError;

    fn try_into(self) -> Result<Message, Self::Error> {
        Ok((TariMessageType::new(BlockchainMessage::UTxs), self).try_into()?)
    }
}

/// The UTxStatusRequestMessage requests the status of the unspent transaction specified by utx_hash
#[derive(Serialize, Deserialize)]
pub struct UTxStatusRequestMessage {
    utx_hash: Vec<u8>,
}

impl TryInto<Message> for UTxStatusRequestMessage {
    type Error = MessageError;

    fn try_into(self) -> Result<Message, Self::Error> {
        Ok((TariMessageType::new(BlockchainMessage::UTxStatusRequest), self).try_into()?)
    }
}

/// The UTxStatusMessage contains all information relating to the specified transaction in the Mempool
#[derive(Serialize, Deserialize)]
pub struct UTxStatusMessage {
    utx_hash: Vec<u8>,
    /* verification status?
     * in which pool?
     * priority? */
}

impl TryInto<Message> for UTxStatusMessage {
    type Error = MessageError;

    fn try_into(self) -> Result<Message, Self::Error> {
        Ok((TariMessageType::new(BlockchainMessage::UTxStatus), self).try_into()?)
    }
}
