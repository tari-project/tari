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

use crate::error::MempoolError;
use crossbeam_channel as channel;
use log::*;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tari_comms::{types::CommsPublicKey, DomainConnector};
use tari_core::transaction::Transaction;
use tari_p2p::{
    services::{
        Service,
        ServiceApiWrapper,
        ServiceContext,
        ServiceControlMessage,
        ServiceError,
        DEFAULT_API_TIMEOUT_MS,
    },
    tari_message::{BlockchainMessage, TariMessageType},
};

const LOG_TARGET: &str = "base_layer::mempool::service";

/// The MempoolService is responsible for managing, verifying and maintaining all unconfirmed transactions that have not
/// yet been included in a block and added to the Tari blockchain. It consists of a Transaction Pool, Pending Pool,
/// Orphan Pool and Reorg Pool.
pub struct MempoolService {
    api: ServiceApiWrapper<MempoolServiceApi, MempoolApiRequest, MempoolApiResult>,
}

impl MempoolService {
    /// Create a new Mempool service
    pub fn new() -> Self {
        Self { api: Self::setup_api() }
    }

    /// Return this services API
    pub fn get_api(&self) -> Arc<MempoolServiceApi> {
        self.api.get_api()
    }

    fn setup_api() -> ServiceApiWrapper<MempoolServiceApi, MempoolApiRequest, MempoolApiResult> {
        let (api_sender, service_receiver) = channel::bounded(0);
        let (service_sender, api_receiver) = channel::bounded(0);

        let api = Arc::new(MempoolServiceApi::new(api_sender, api_receiver));
        ServiceApiWrapper::new(service_receiver, service_sender, api)
    }

    /// Send a sync request to neighbouring peers to download the content of their Mempools
    fn send_sync_request(&self) -> Result<(), MempoolError> {
        // TODO: In the download request send a list of known tx hashes to the neighbouring nodes so they can respond
        // with all the txs that are unknown to this node. Make use of DHT Service to send a message to
        // neighbouring peers

        Ok(())
    }

    /// Received a sync request from a neighbouring peer to download the content of the local Mempool
    fn receive_sync_request(&mut self, _connector: &DomainConnector<'static>) -> Result<(), MempoolError> {
        // TODO: The DHT Service should check that the requesting node is a neighbouring node.

        // TODO: Compile a list of unconfirmed txs, that are not in the received list of hashes
        // (known_tx_hashes), and then send a sync_reply to the requesting node.

        Ok(())
    }

    /// Received a sync reply, corresponding to a previously sent sync request
    fn receive_sync_reply(&mut self, _connector: &DomainConnector<'static>) -> Result<(), MempoolError> {
        // TODO Check each received transaction and add to the correct pool (Transaction Pool, Pending Pool, Orphan
        // Pool)

        Ok(())
    }

    /// Request the Mempool stats of a specific node
    fn send_stats_request(&self, _dest_public_key: CommsPublicKey) -> Result<(), MempoolError> {
        // TODO: Construct and send the mempool stats request to the specified node

        Ok(())
    }

    /// Receive Mempool stats request from neighbouring node
    fn receive_stats_request(&mut self, _connector: &DomainConnector<'static>) -> Result<(), MempoolError> {
        // TODO: The DHT Service should check that the requesting node is a neighbouring node.

        // TODO: Construct and send a Stats replay message with the following info
        // The number of unconfirmed transactions
        // The number of orphaned transactions
        // The number of timelocked transactions
        // The current size of the mempool (in transaction weight)
        // Maybe the minimum tx fee that this node will accept

        Ok(())
    }

    /// Receive the Mempool stats from another node
    fn receive_stats_reply(&self, _connector: &DomainConnector<'static>) -> Result<(), MempoolError> {
        // TODO: Check that a request was sent and store the received stats, or provide it to internal function that
        // requested it

        Ok(())
    }

    /// Send a set of unconfirmed transactions to the network
    fn send_utxs(&self, _utxs: Vec<Transaction>) -> Result<(), MempoolError> {
        // TODO: This function can be used to submit a new transaction or set of transactions to the network
        // TODO: Check that transactions are ranked by the transaction priority metric (of interest to miners)
        // TODO: Use DHT Service to propagate the utxs to peers

        Ok(())
    }

    /// Receive a bundle of unconfirmed transactions
    fn receive_utxs(&mut self, _connector: &DomainConnector<'static>) -> Result<(), MempoolError> {
        // TODO: Check each received transaction and add to the correct pool (Transaction Pool, Pending Pool, Orphan
        // Pool)
        // TODO: Use DHT Service to propagate the utxs to peers

        Ok(())
    }

    /// Request the status of a specific transaction from another Mempool
    fn send_utx_status_request(
        &self,
        _dest_public_key: CommsPublicKey,
        _utx_hash: Vec<u8>,
    ) -> Result<(), MempoolError>
    {
        // TODO: Construct a StatsRequestMessage and send to peer

        Ok(())
    }

    /// Another Node has requested the status of a specific unspent transaction
    fn receive_utx_status_request(&self, _connector: &DomainConnector<'static>) -> Result<(), MempoolError> {
        // TODO: Find the transaction, construct a StatsReplyMessage and send to requesting peer

        Ok(())
    }

    /// Receive the status of a specific unspent transactions from another Mempool
    fn receive_utx_status_reply(&self, _connector: &DomainConnector<'static>) -> Result<(), MempoolError> {
        // TODO: Check that a request was sent and store received utx status or provide it to internal function that
        // requested it

        Ok(())
    }

    /// Returns a set of mineable transactions
    fn mineable_txs(&self) -> Result<Vec<Transaction>, MempoolError> {
        // TODO: compile a set of highest priority mineable transactions.
        // Move these transactions to the reorg pool, these transactions will be moved back to the unspent transaction
        // pool if they don't appear in a base layer block in a short period of time.

        Ok(Vec::new())
    }

    /// This handler is called when the Service executor loops receives an API request
    fn handle_api_message(&self, msg: MempoolApiRequest) -> Result<(), ServiceError> {
        trace!(
            target: LOG_TARGET,
            "[{}] Received API message: {:?}",
            self.get_name(),
            msg
        );
        let resp = match msg {
            MempoolApiRequest::SendSyncRequest => self.send_sync_request().map(|_| MempoolApiResponse::SyncRequestSent),
            MempoolApiRequest::SendStatsRequest(dest_public_key) => self
                .send_stats_request(dest_public_key)
                .map(|_| MempoolApiResponse::StatsRequestSent),
            MempoolApiRequest::SendUTxs(utxs) => self.send_utxs(utxs).map(|_| MempoolApiResponse::UTxsSent),
            MempoolApiRequest::SendUTxStatusRequest(dest_public_key, utx_hash) => self
                .send_utx_status_request(dest_public_key, utx_hash)
                .map(|_| MempoolApiResponse::UTxStatusRequestSent),

            MempoolApiRequest::RetrieveMineableTxs => Ok(MempoolApiResponse::MineableTxsResponse(
                self.mineable_txs().map_err(ServiceError::internal_service_error())?,
            )),
        };

        trace!(target: LOG_TARGET, "[{}] Replying to API: {:?}", self.get_name(), resp);
        self.api
            .send_reply(resp)
            .map_err(ServiceError::internal_service_error())
    }
}

/// The Domain Service trait implementation for the MempoolService
impl Service for MempoolService {
    fn get_name(&self) -> String {
        "mempool".to_string()
    }

    fn get_message_types(&self) -> Vec<TariMessageType> {
        vec![
            BlockchainMessage::MempoolSyncRequest.into(),
            BlockchainMessage::MempoolSync.into(),
            BlockchainMessage::MempoolStatsRequest.into(),
            BlockchainMessage::MempoolStats.into(),
            BlockchainMessage::UTxs.into(),
            BlockchainMessage::UTxStatusRequest.into(),
            BlockchainMessage::UTxStatus.into(),
        ]
    }

    fn execute(&mut self, context: ServiceContext) -> Result<(), ServiceError> {
        let connector_sync_request = context
            .create_connector(&BlockchainMessage::MempoolSyncRequest.into())
            .map_err(|err| {
                ServiceError::ServiceInitializationFailed(format!("Failed to create connector for service: {}", err))
            })?;

        let connector_sync_reply = context
            .create_connector(&BlockchainMessage::MempoolSync.into())
            .map_err(|err| {
                ServiceError::ServiceInitializationFailed(format!("Failed to create connector for service: {}", err))
            })?;

        let connector_stats_request = context
            .create_connector(&BlockchainMessage::MempoolStatsRequest.into())
            .map_err(|err| {
                ServiceError::ServiceInitializationFailed(format!("Failed to create connector for service: {}", err))
            })?;

        let connector_stats_reply = context
            .create_connector(&BlockchainMessage::MempoolStats.into())
            .map_err(|err| {
                ServiceError::ServiceInitializationFailed(format!("Failed to create connector for service: {}", err))
            })?;

        let connector_utxs = context
            .create_connector(&BlockchainMessage::UTxs.into())
            .map_err(|err| {
                ServiceError::ServiceInitializationFailed(format!("Failed to create connector for service: {}", err))
            })?;

        let connector_utx_status_request = context
            .create_connector(&BlockchainMessage::UTxStatusRequest.into())
            .map_err(|err| {
                ServiceError::ServiceInitializationFailed(format!("Failed to create connector for service: {}", err))
            })?;

        let connector_utx_status_reply =
            context
                .create_connector(&BlockchainMessage::UTxStatus.into())
                .map_err(|err| {
                    ServiceError::ServiceInitializationFailed(format!(
                        "Failed to create connector for service: {}",
                        err
                    ))
                })?;

        debug!(target: LOG_TARGET, "Starting Mempool Service executor");
        loop {
            if let Some(msg) = context.get_control_message(Duration::from_millis(5)) {
                match msg {
                    ServiceControlMessage::Shutdown => break,
                }
            }

            match self.receive_sync_request(&connector_sync_request) {
                Ok(_) => {},
                Err(err) => {
                    error!(target: LOG_TARGET, "Mempool service had error: {:?}", err);
                },
            }

            match self.receive_sync_reply(&connector_sync_reply) {
                Ok(_) => {},
                Err(err) => {
                    error!(target: LOG_TARGET, "Mempool service had error: {:?}", err);
                },
            }

            match self.receive_stats_request(&connector_stats_request) {
                Ok(_) => {},
                Err(err) => {
                    error!(target: LOG_TARGET, "Mempool service had error: {:?}", err);
                },
            }

            match self.receive_stats_reply(&connector_stats_reply) {
                Ok(_) => {},
                Err(err) => {
                    error!(target: LOG_TARGET, "Mempool service had error: {:?}", err);
                },
            }

            match self.receive_utxs(&connector_utxs) {
                Ok(_) => {},
                Err(err) => {
                    error!(target: LOG_TARGET, "Mempool service had error: {:?}", err);
                },
            }

            match self.receive_utx_status_request(&connector_utx_status_request) {
                Ok(_) => {},
                Err(err) => {
                    error!(target: LOG_TARGET, "Mempool service had error: {:?}", err);
                },
            }

            match self.receive_utx_status_reply(&connector_utx_status_reply) {
                Ok(_) => {},
                Err(err) => {
                    error!(target: LOG_TARGET, "Mempool service had error: {:?}", err);
                },
            }

            // TODO: Check/Update Transaction Pool, Pending Pool, Orphan Pool and Reorg Pool

            if let Some(msg) = self
                .api
                .recv_timeout(Duration::from_millis(5))
                .map_err(ServiceError::internal_service_error())?
            {
                self.handle_api_message(msg)?;
            }
        }

        Ok(())
    }
}

/// API Request enum
#[derive(Debug)]
pub enum MempoolApiRequest {
    /// Send a Mempool download request to another Node
    SendSyncRequest,
    /// Send a Mempool stats request to another Node
    SendStatsRequest(CommsPublicKey),
    /// Send a set of unconfirmed transactions to the network
    SendUTxs(Vec<Transaction>),
    /// Send a status request for a specific transaction
    SendUTxStatusRequest(CommsPublicKey, Vec<u8>),
    /// Retrieve a set of mineable transactions for block construction
    RetrieveMineableTxs,
}

/// API Response enum
#[derive(Debug)]
pub enum MempoolApiResponse {
    SyncRequestSent,
    StatsRequestSent,
    UTxsSent,
    UTxStatusRequestSent,
    MineableTxsResponse(Vec<Transaction>),
}

/// Result for all API requests
pub type MempoolApiResult = Result<MempoolApiResponse, MempoolError>;

/// The Mempool service public API that other services and application will use to interact with this service.
/// The requests and responses are transmitted via channels into the Service Executor thread where this service is
/// running
pub struct MempoolServiceApi {
    sender: channel::Sender<MempoolApiRequest>,
    receiver: channel::Receiver<MempoolApiResult>,
    mutex: Mutex<()>,
    timeout: Duration,
}

impl MempoolServiceApi {
    fn new(sender: channel::Sender<MempoolApiRequest>, receiver: channel::Receiver<MempoolApiResult>) -> Self {
        Self {
            sender,
            receiver,
            mutex: Mutex::new(()),
            timeout: Duration::from_millis(DEFAULT_API_TIMEOUT_MS),
        }
    }

    pub fn sync(&self) -> Result<(), MempoolError> {
        self.send_recv(MempoolApiRequest::SendSyncRequest)
            .and_then(|resp| match resp {
                MempoolApiResponse::SyncRequestSent => Ok(()),
                _ => Err(MempoolError::UnexpectedApiResponse),
            })
    }

    pub fn remote_stats(&self, dest_public_key: CommsPublicKey) -> Result<(), MempoolError> {
        self.send_recv(MempoolApiRequest::SendStatsRequest(dest_public_key))
            .and_then(|resp| match resp {
                MempoolApiResponse::StatsRequestSent => Ok(()),
                _ => Err(MempoolError::UnexpectedApiResponse),
            })
    }

    pub fn send(&self, utxs: Vec<Transaction>) -> Result<(), MempoolError> {
        self.send_recv(MempoolApiRequest::SendUTxs(utxs))
            .and_then(|resp| match resp {
                MempoolApiResponse::UTxsSent => Ok(()),
                _ => Err(MempoolError::UnexpectedApiResponse),
            })
    }

    pub fn remote_utx_status(&self, dest_public_key: CommsPublicKey, utx_hash: Vec<u8>) -> Result<(), MempoolError> {
        self.send_recv(MempoolApiRequest::SendUTxStatusRequest(dest_public_key, utx_hash))
            .and_then(|resp| match resp {
                MempoolApiResponse::UTxStatusRequestSent => Ok(()),
                _ => Err(MempoolError::UnexpectedApiResponse),
            })
    }

    pub fn mineable_txs(&self) -> Result<Vec<Transaction>, MempoolError> {
        self.send_recv(MempoolApiRequest::RetrieveMineableTxs)
            .and_then(|resp| match resp {
                MempoolApiResponse::MineableTxsResponse(txs) => Ok(txs),
                _ => Err(MempoolError::UnexpectedApiResponse),
            })
    }

    fn send_recv(&self, msg: MempoolApiRequest) -> MempoolApiResult {
        self.lock(|| -> MempoolApiResult {
            self.sender.send(msg).map_err(|_| MempoolError::ApiSendFailed)?;
            self.receiver
                .recv_timeout(self.timeout)
                .map_err(|_| MempoolError::ApiReceiveFailed)?
        })
    }

    fn lock<F, T>(&self, func: F) -> T
    where F: FnOnce() -> T {
        let lock = acquire_lock!(self.mutex);
        let res = func();
        drop(lock);
        res
    }
}
