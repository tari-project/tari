// Copyright 2025. The Tari Project
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

use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::Error;
use async_trait::async_trait;
use itertools::Itertools;
use minotari_node_wallet_client::BaseNodeWalletClient;
use minotari_wallet::client::http_client_factory::HttpClientFactory;
use tari_shutdown::ShutdownSignal;
use tari_transaction_components::{
    rpc::models::{
        self,
        BlockHeader,
        BlockUtxoInfo,
        FeePerGramStat,
        GenerateKernelMerkleProofResponse,
        GetUtxosDeletedInfoResponse,
        GetUtxosMinedInfoResponse,
        SyncUtxosByBlockResponseV0,
        TxSubmissionResponse,
    },
    transaction_components::{Transaction, TransactionOutput},
};
use tari_utilities::ByteArray;
use tokio::sync::{RwLock, mpsc};
use url::Url;

use crate::support::comms_rpc::UtxosByBlock;

#[derive(Default)]
struct State {
    utxos_by_block: HashMap<u64, UtxosByBlock>,
    blocks: HashMap<u64, tari_node_components::blocks::BlockHeader>,
    tip_info: Option<models::TipInfoResponse>,
    online: bool,
    http_address: Option<String>,
    last_request_latency: Option<Duration>,
}

impl State {
    fn set_utxos_by_block(&mut self, utxos_by_block: Vec<UtxosByBlock>) {
        self.utxos_by_block = utxos_by_block.into_iter().map(|ub| (ub.height, ub)).collect();
    }

    fn set_blocks(&mut self, blocks: HashMap<u64, tari_node_components::blocks::BlockHeader>) {
        self.blocks = blocks;
    }

    fn set_tip_info(&mut self, tip_info: models::TipInfoResponse) {
        self.tip_info = Some(tip_info);
    }

    fn set_online_status(&mut self, is_online: bool) {
        self.online = is_online;
    }

    fn set_http_address(&mut self, http_address: String) {
        self.http_address = Some(http_address);
    }

    fn set_last_request_latency(&mut self, last_request_latency: Duration) {
        self.last_request_latency = Some(last_request_latency);
    }
}

#[derive(Clone, Default)]
pub struct HttpBaseNodeMock {
    state: Arc<RwLock<State>>,
}

impl HttpBaseNodeMock {
    pub async fn set_utxos_by_block(&self, utxos_by_block: Vec<UtxosByBlock>) -> Result<(), Error> {
        let mut s = self.state.write().await;
        s.set_utxos_by_block(utxos_by_block);
        Ok(())
    }

    pub async fn set_blocks(
        &self,
        blocks: HashMap<u64, tari_node_components::blocks::BlockHeader>,
    ) -> Result<(), Error> {
        let mut state = self.state.write().await;
        state.set_blocks(blocks);

        Ok(())
    }

    pub async fn set_tip_info(&self, tip_info: models::TipInfoResponse) -> Result<(), Error> {
        let mut state = self.state.write().await;
        state.set_tip_info(tip_info);
        Ok(())
    }

    pub async fn set_online_status(&self, is_online: bool) -> Result<(), Error> {
        let mut state = self.state.write().await;
        state.set_online_status(is_online);
        Ok(())
    }

    pub async fn set_http_address(&self, http_address: String) -> Result<(), Error> {
        let mut state = self.state.write().await;
        state.set_http_address(http_address);
        Ok(())
    }

    pub async fn set_last_request_latency(&self, last_request_latency: Duration) -> Result<(), Error> {
        let mut state = self.state.write().await;
        state.set_last_request_latency(last_request_latency);
        Ok(())
    }
}

#[async_trait]
impl BaseNodeWalletClient for HttpBaseNodeMock {
    async fn get_address(&self) -> String {
        let state = self.state.read().await;
        if let Some(http_address) = &state.http_address {
            http_address.clone()
        } else {
            String::new()
        }
    }

    async fn is_online(&self) -> bool {
        let state = self.state.read().await;
        if state.tip_info.is_some() { state.online } else { false }
    }

    async fn get_last_request_latency(&self) -> Option<Duration> {
        let state = self.state.read().await;
        state.last_request_latency
    }

    async fn get_utxos_mined_info(
        &self,
        _hashes: Vec<Vec<u8>>,
        _version: u32,
    ) -> Result<GetUtxosMinedInfoResponse, Error> {
        todo!()
    }

    async fn fetch_utxo(&self, _hash: Vec<u8>) -> Result<Option<TransactionOutput>, Error> {
        todo!()
    }

    async fn query_deleted_utxos(
        &self,
        _hashes: Vec<Vec<u8>>,
        _must_include_header: Vec<u8>,
    ) -> Result<GetUtxosDeletedInfoResponse, Error> {
        todo!()
    }

    async fn submit_transaction(&self, _transaction: Transaction) -> Result<TxSubmissionResponse, Error> {
        Ok(TxSubmissionResponse {
            accepted: true,
            rejection_reason: models::TxSubmissionRejectionReason::None,
            is_synced: true,
        })
    }

    async fn transaction_query(
        &self,
        _excess_sig_nonce: Vec<u8>,
        _excess_sig_sig: Vec<u8>,
    ) -> Result<models::TxQueryResponse, Error> {
        todo!()
    }

    async fn get_mempool_fee_per_gram_stats(&self, _count: u64) -> Result<FeePerGramStat, Error> {
        todo!()
    }

    async fn get_tip_info(&self) -> Result<models::TipInfoResponse, Error> {
        let state = self.state.read().await;
        if let Some(tip_info) = &state.tip_info {
            Ok(tip_info.clone())
        } else {
            Err(Error::msg("Tip info not set"))
        }
    }

    async fn get_header_by_height(&self, height: u64) -> Result<Option<BlockHeader>, Error> {
        let state = self.state.read().await;
        if let Some(header) = state.blocks.get(&height) {
            Ok(Some(BlockHeader::from(header.clone())))
        } else {
            Ok(None)
        }
    }

    async fn get_height_at_time(&self, epoch_time: u64) -> Result<u64, Error> {
        let state = self.state.read().await;
        let mut last_header = 0;
        let headers = state.blocks.values().sorted_by(|a, b| a.height.cmp(&b.height));
        for header in headers {
            if header.timestamp.as_u64() >= epoch_time {
                return Ok(last_header);
            }
            last_header = header.height;
        }
        Ok(last_header)
    }

    async fn get_utxos_by_block(&self, header_hash: Vec<u8>) -> Result<models::GetUtxosByBlockResponse, Error> {
        let state = self.state.read().await;
        let mut utxos = Vec::new();
        for ub in state.utxos_by_block.values() {
            if ub.header_hash.to_vec() == header_hash {
                utxos.extend(ub.utxos.iter().cloned());
            }
        }
        let header = state.blocks.values().find(|h| h.hash().to_vec() == header_hash);

        let res = if let Some(header) = header {
            models::GetUtxosByBlockResponse {
                header_hash: header.hash().to_vec(),
                height: header.height,
                outputs: utxos,
                mined_timestamp: header.timestamp.as_u64(),
            }
        } else {
            return Err(Error::msg("Header not found for the given hash"));
        };
        Ok(res)
    }

    async fn sync_utxos_by_block(
        &self,
        start_header_hash: Vec<u8>,
        shutdown: ShutdownSignal,
    ) -> Result<mpsc::Receiver<Result<SyncUtxosByBlockResponseV0, Error>>, Error> {
        let (tx, rx) = mpsc::channel(100);
        let state2 = self.state.read().await;

        let start_height = state2
            .blocks
            .values()
            .find(|b| b.hash().to_vec() == start_header_hash)
            .map_or(0, |b| b.height);

        let end_height = state2.tip_info.as_ref().map_or(0, |tip| {
            tip.metadata.as_ref().map_or(0, |meta| meta.best_block_height())
        });
        let state = self.state.clone();
        tokio::spawn(async move {
            let state = state.read().await;
            let mut blocks = vec![];
            let page_size = 5;

            for height in start_height..=end_height {
                if shutdown.is_triggered() {
                    break;
                }
                if let Some(ub) = state.utxos_by_block.get(&height) {
                    let block_header = state.blocks.get(&height).cloned();
                    if let Some(header) = block_header {
                        blocks.push(BlockUtxoInfo {
                            header_hash: header.hash().to_vec(),
                            height: header.height,
                            outputs: ub
                                .utxos
                                .iter()
                                .map(|o| models::MinimalUtxoSyncInfo {
                                    output_hash: o.hash().to_vec(),
                                    commitment: o.commitment.as_bytes().to_vec(),
                                    encrypted_data: o.encrypted_data.to_byte_vec(),
                                    sender_offset_public_key: o.sender_offset_public_key.to_vec(),
                                })
                                .collect(),
                            inputs: Vec::new(),
                            mined_timestamp: header.timestamp.as_u64(),
                        });
                    }
                }
                if blocks.len() >= page_size || height == end_height {
                    let has_next_page = height < end_height;
                    let response = SyncUtxosByBlockResponseV0 {
                        blocks: blocks.clone(),
                        has_next_page,
                        next_header_to_scan: vec![],
                    };
                    blocks.clear();

                    if tx.send(Ok(response)).await.is_err() {
                        break; // Channel closed
                    }
                }
            }
        });

        Ok(rx)
    }

    async fn get_kernel_merkle_proof(
        &self,
        _excess_sig_nonce: &[u8],
        _excess_sig: &[u8],
    ) -> Result<GenerateKernelMerkleProofResponse, Error> {
        panic!("Not implemented")
    }
}

#[derive(Clone)]
pub struct MockHttpClientFactory {
    mock: HttpBaseNodeMock,
}

impl MockHttpClientFactory {
    pub fn get_client(&self) -> HttpBaseNodeMock {
        self.mock.clone()
    }
}

impl Default for MockHttpClientFactory {
    fn default() -> Self {
        Self {
            mock: HttpBaseNodeMock {
                state: Arc::new(RwLock::new(State::default())),
            },
        }
    }
}

impl HttpClientFactory for MockHttpClientFactory {
    type Client = HttpBaseNodeMock;

    fn new(_node_url: Url, _seed_url: Url) -> Self {
        Self::default()
    }

    fn create_http_client(&self) -> Self::Client {
        self.mock.clone()
    }
}
