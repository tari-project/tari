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
    mempool::{
        test_utils::mock::{create_mempool_service_mock, MempoolMockState},
        MempoolRpcService,
    },
    test_helpers::create_peer_manager,
};
use tari_comms::protocol::rpc::mock::RpcRequestMock;
use tempfile::{tempdir, TempDir};

fn setup() -> (MempoolRpcService, MempoolMockState, RpcRequestMock, TempDir) {
    let tmp = tempdir().unwrap();
    let peer_manager = create_peer_manager(&tmp);
    let request_mock = RpcRequestMock::new(peer_manager.clone());
    let (handle, state) = create_mempool_service_mock();
    let service = MempoolRpcService::new(handle);
    (service, state, request_mock, tmp)
}

mod get_stats {
    use super::*;
    use crate::mempool::{MempoolService, StatsResponse};

    #[tokio_macros::test_basic]
    async fn it_returns_the_stats() {
        let (service, mempool, req_mock, _tmpdir) = setup();
        let expected_stats = StatsResponse {
            total_txs: 1,
            unconfirmed_txs: 2,
            orphan_txs: 3,
            timelocked_txs: 4,
            published_txs: 5,
            total_weight: 6,
        };
        mempool.set_get_stats_response(expected_stats.clone()).await;

        let resp = service.get_stats(req_mock.request_no_context(())).await.unwrap();
        let stats = resp.into_message();
        assert_eq!(stats, expected_stats.into());
        assert_eq!(mempool.get_call_count(), 1);
    }
}

mod get_state {
    use super::*;
    use crate::mempool::{MempoolService, StateResponse};

    #[tokio_macros::test_basic]
    async fn it_returns_the_state() {
        let (service, mempool, req_mock, _tmpdir) = setup();
        let expected_state = StateResponse {
            unconfirmed_pool: vec![],
            orphan_pool: vec![],
            pending_pool: vec![],
            reorg_pool: vec![],
        };
        mempool.set_get_state_response(expected_state.clone()).await;

        let resp = service.get_state(req_mock.request_no_context(())).await.unwrap();
        let stats = resp.into_message();
        assert_eq!(stats, expected_state.into());
        assert_eq!(mempool.get_call_count(), 1);
    }
}

mod get_tx_state_by_excess_sig {
    use super::*;
    use crate::{
        mempool::{MempoolService, TxStorageResponse},
        proto::types::Signature,
        tari_utilities::ByteArray,
    };
    use tari_comms::protocol::rpc::RpcStatusCode;
    use tari_crypto::ristretto::{RistrettoPublicKey, RistrettoSecretKey};
    use tari_test_utils::unpack_enum;

    #[tokio_macros::test_basic]
    async fn it_returns_the_storage_status() {
        let (service, mempool, req_mock, _tmpdir) = setup();
        let expected = TxStorageResponse::UnconfirmedPool;
        mempool.set_get_tx_by_excess_sig_stats_response(expected.clone()).await;

        let public_nonce = RistrettoPublicKey::default();
        let signature = RistrettoSecretKey::default();

        let sig = Signature {
            public_nonce: public_nonce.to_vec(),
            signature: signature.to_vec(),
        };
        let resp = service
            .get_transaction_state_by_excess_sig(req_mock.request_no_context(sig))
            .await
            .unwrap();
        let resp = resp.into_message();
        assert_eq!(resp, expected.into());
        assert_eq!(mempool.get_call_count(), 1);
    }

    #[tokio_macros::test_basic]
    async fn it_errors_on_invalid_signature() {
        let (service, _, req_mock, _tmpdir) = setup();
        let status = service
            .get_transaction_state_by_excess_sig(req_mock.request_no_context(Default::default()))
            .await
            .unwrap_err();

        unpack_enum!(RpcStatusCode::BadRequest = status.status_code());
    }
}

mod submit_transaction {
    use super::*;
    use crate::{
        mempool::{MempoolService, TxStorageResponse},
        proto::types::{AggregateBody, BlindingFactor, Transaction},
        tari_utilities::ByteArray,
    };
    use tari_comms::protocol::rpc::RpcStatusCode;
    use tari_crypto::ristretto::RistrettoSecretKey;
    use tari_test_utils::unpack_enum;

    #[tokio_macros::test_basic]
    async fn it_submits_transaction() {
        let (service, mempool, req_mock, _tmpdir) = setup();
        let expected = TxStorageResponse::UnconfirmedPool;
        mempool.set_submit_transaction_response(expected.clone()).await;
        let txn = Transaction {
            offset: Some(BlindingFactor {
                data: RistrettoSecretKey::default().to_vec(),
            }),
            body: Some(AggregateBody {
                inputs: vec![],
                outputs: vec![],
                kernels: vec![],
            }),
        };
        let resp = service
            .submit_transaction(req_mock.request_with_context(Default::default(), txn))
            .await
            .unwrap();
        let resp = resp.into_message();
        assert_eq!(resp, expected.into());
        assert_eq!(mempool.get_call_count(), 1);
    }

    #[tokio_macros::test_basic]
    async fn it_errors_on_invalid_transaction() {
        let (service, _, req_mock, _tmpdir) = setup();
        let status = service
            .submit_transaction(req_mock.request_with_context(Default::default(), Default::default()))
            .await
            .unwrap_err();

        unpack_enum!(RpcStatusCode::BadRequest = status.status_code());
    }
}
