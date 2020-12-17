//  Copyright 2020, The Tari Project
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

use crate::{
    base_node::rpc::BaseNodeWalletService,
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend},
    mempool::service::MempoolHandle,
    proto::generated::{
        base_node::{TxLocation, TxQueryResponse, TxSubmissionResponse},
        types::{Signature as SignatureProto, Transaction as TransactionProto},
    },
};

use crate::{
    mempool::TxStorageResponse,
    proto::generated::base_node::TxSubmissionRejectionReason,
    transactions::{transaction::Transaction, types::Signature},
};
use std::convert::TryFrom;
use tari_comms::protocol::rpc::{Request, Response, RpcStatus};

const LOG_TARGET: &str = "c::base_node::rpc";

pub struct BaseNodeWalletRpcService<B> {
    db: AsyncBlockchainDb<B>,
    mempool: MempoolHandle,
}

impl<B: BlockchainBackend + 'static> BaseNodeWalletRpcService<B> {
    pub fn new(db: AsyncBlockchainDb<B>, mempool: MempoolHandle) -> Self {
        Self { db, mempool }
    }

    #[inline]
    fn db(&self) -> AsyncBlockchainDb<B> {
        self.db.clone()
    }

    #[inline]
    pub fn mempool(&self) -> MempoolHandle {
        self.mempool.clone()
    }
}

#[tari_comms::async_trait]
impl<B: BlockchainBackend + 'static> BaseNodeWalletService for BaseNodeWalletRpcService<B> {
    async fn submit_transaction(
        &self,
        request: Request<TransactionProto>,
    ) -> Result<Response<TxSubmissionResponse>, RpcStatus>
    {
        let message = request.into_message();
        let transaction =
            Transaction::try_from(message).map_err(|_| RpcStatus::bad_request("Transaction was invalid"))?;
        let mut mempool = self.mempool();
        let response = match mempool
            .submit_transaction(transaction.clone())
            .await
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?
        {
            TxStorageResponse::UnconfirmedPool => TxSubmissionResponse {
                accepted: true,
                rejection_reason: TxSubmissionRejectionReason::None.into(),
            },

            TxStorageResponse::NotStoredOrphan => TxSubmissionResponse {
                accepted: false,
                rejection_reason: TxSubmissionRejectionReason::Orphan.into(),
            },
            TxStorageResponse::NotStoredTimeLocked => TxSubmissionResponse {
                accepted: false,
                rejection_reason: TxSubmissionRejectionReason::TimeLocked.into(),
            },

            TxStorageResponse::NotStored => TxSubmissionResponse {
                accepted: false,
                rejection_reason: TxSubmissionRejectionReason::ValidationFailed.into(),
            },
            TxStorageResponse::NotStoredAlreadySpent | TxStorageResponse::ReorgPool => {
                // Is this transaction a double spend or has this transaction been mined?
                match transaction.first_kernel_excess_sig() {
                    None => TxSubmissionResponse {
                        accepted: false,
                        rejection_reason: TxSubmissionRejectionReason::DoubleSpend.into(),
                    },
                    Some(s) => {
                        // Check to see if the kernel exists in the blockchain db in which case this exact transaction
                        // already exists in the chain, otherwise it is a double spend
                        let db = self.db();
                        match db
                            .fetch_kernel_by_excess_sig(s.clone())
                            .await
                            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?
                        {
                            None => TxSubmissionResponse {
                                accepted: false,
                                rejection_reason: TxSubmissionRejectionReason::DoubleSpend.into(),
                            },
                            Some(_) => TxSubmissionResponse {
                                accepted: false,
                                rejection_reason: TxSubmissionRejectionReason::AlreadyMined.into(),
                            },
                        }
                    },
                }
            },
        };
        Ok(Response::new(response))
    }

    async fn transaction_query(
        &self,
        request: Request<SignatureProto>,
    ) -> Result<Response<TxQueryResponse>, RpcStatus>
    {
        let message = request.into_message();
        let signature = Signature::try_from(message).map_err(|_| RpcStatus::bad_request("Signature was invalid"))?;

        let db = self.db();
        match db
            .fetch_kernel_by_excess_sig(signature.clone())
            .await
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?
        {
            None => (),
            Some((_, block_hash)) => {
                match db
                    .fetch_header_by_block_hash(block_hash.clone())
                    .await
                    .map_err(RpcStatus::log_internal_error(LOG_TARGET))?
                {
                    None => (),
                    Some(header) => {
                        let chain_meta_data = db
                            .get_chain_metadata()
                            .await
                            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?;
                        let confirmations = chain_meta_data.height_of_longest_chain() - header.height;
                        let response = TxQueryResponse {
                            location: TxLocation::Mined as i32,
                            block_hash: Some(block_hash),
                            confirmations,
                        };
                        return Ok(Response::new(response));
                    },
                }
            },
        };

        // If not in a block then check the mempool
        let mut mempool = self.mempool();
        let mempool_response = match mempool
            .get_tx_state_by_excess_sig(signature.clone())
            .await
            .map_err(RpcStatus::log_internal_error(LOG_TARGET))?
        {
            TxStorageResponse::UnconfirmedPool => TxQueryResponse {
                location: TxLocation::InMempool as i32,
                block_hash: None,
                confirmations: 0,
            },
            TxStorageResponse::ReorgPool |
            TxStorageResponse::NotStoredOrphan |
            TxStorageResponse::NotStoredTimeLocked |
            TxStorageResponse::NotStoredAlreadySpent |
            TxStorageResponse::NotStored => TxQueryResponse {
                location: TxLocation::NotStored as i32,
                block_hash: None,
                confirmations: 0,
            },
        };

        Ok(Response::new(mempool_response))
    }
}
