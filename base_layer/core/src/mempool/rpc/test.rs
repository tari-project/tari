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

use tari_comms::protocol::rpc::mock::RpcRequestMock;

use crate::{
    mempool::{
        MempoolRpcService,
        test_utils::mock::{MempoolMockState, create_mempool_service_mock},
    },
    test_helpers::create_peer_manager,
};

fn setup() -> (MempoolRpcService, MempoolMockState, RpcRequestMock) {
    let peer_manager = create_peer_manager();
    let request_mock = RpcRequestMock::new(peer_manager);
    let (handle, state) = create_mempool_service_mock();
    let service = MempoolRpcService::new(handle);
    (service, state, request_mock)
}

mod get_stats {
    use super::*;
    use crate::mempool::{MempoolService, StatsResponse};

    #[tokio::test]
    async fn it_returns_the_stats() {
        let (service, mempool, req_mock) = setup();
        let expected_stats = StatsResponse {
            unconfirmed_txs: 2,

            reorg_txs: 5,
            unconfirmed_weight: 6,
        };
        mempool.set_get_stats_response(expected_stats.clone()).await;

        let resp = service.get_stats(req_mock.request_no_context(())).await.unwrap();
        let stats = resp.into_message();
        assert_eq!(stats, expected_stats.into());
        assert_eq!(mempool.get_call_count(), 1);
    }
}

mod retired_get_state {
    use std::sync::Arc;

    use tari_comms::{
        peer_manager::PeerFeatures,
        protocol::{
            ProtocolId,
            rpc::{NamedProtocolService, RPC_MAX_FRAME_SIZE, RpcClient, RpcError, RpcStatusCode, mock::MockRpcServer},
        },
        test_utils::node_identity::build_node_identity,
    };

    use crate::{
        mempool::{
            MempoolRpcClient,
            StatsResponse,
            rpc::create_mempool_rpc_service,
            test_utils::mock::create_mempool_service_mock,
        },
        proto,
    };

    #[tokio::test]
    async fn method_2_is_unsupported_and_the_session_stays_usable() {
        let (handle, mempool) = create_mempool_service_mock();
        let expected_stats = StatsResponse {
            unconfirmed_txs: 1,
            reorg_txs: 2,
            unconfirmed_weight: 3,
        };
        mempool.set_get_stats_response(expected_stats.clone()).await;

        let server_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
        let client_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
        let protocol = ProtocolId::from_static(MempoolRpcClient::PROTOCOL_NAME);
        let mut server = MockRpcServer::new(create_mempool_rpc_service(handle), server_identity.clone());
        server.serve();
        let mut conn = server
            .create_connection(client_identity.to_peer(), protocol.clone())
            .await;
        let framed = conn.open_framed_substream(&protocol, RPC_MAX_FRAME_SIZE).await.unwrap();
        let mut client = RpcClient::connect(
            Default::default(),
            server_identity.node_id().clone(),
            framed,
            protocol,
            None,
            Arc::new(true.into()),
        )
        .await
        .unwrap();

        let err = client
            .request_response::<_, proto::mempool::StatsResponse, _>((), 2u32, usize::MAX)
            .await
            .unwrap_err();
        let RpcError::RequestFailed(status) = err else {
            panic!("expected a request failure, got {err:?}");
        };
        assert_eq!(status.as_status_code(), RpcStatusCode::UnsupportedMethod);

        let stats = MempoolRpcClient::from(client).get_stats().await.unwrap();
        assert_eq!(stats, expected_stats.into());
    }
}

mod get_tx_state_by_excess_sig {
    use tari_common_types::types::UncompressedPublicKey;
    use tari_comms::protocol::rpc::RpcStatusCode;
    use tari_crypto::ristretto::RistrettoSecretKey;
    use tari_test_utils::unpack_enum;
    use tari_utilities::ByteArray;

    use super::*;
    use crate::{
        mempool::{MempoolService, TxStorageResponse},
        proto::types::Signature,
    };

    #[tokio::test]
    async fn it_returns_the_storage_status() {
        let (service, mempool, req_mock) = setup();
        let expected = TxStorageResponse::UnconfirmedPool;
        mempool.set_get_tx_by_excess_sig_stats_response(expected.clone()).await;

        let public_nonce = UncompressedPublicKey::default();
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

    #[tokio::test]
    async fn it_errors_on_invalid_signature() {
        let (service, _, req_mock) = setup();
        let status = service
            .get_transaction_state_by_excess_sig(req_mock.request_no_context(Default::default()))
            .await
            .unwrap_err();

        unpack_enum!(RpcStatusCode::BadRequest = status.as_status_code());
    }
}

mod submit_transaction {
    use tari_comms::protocol::rpc::RpcStatusCode;
    use tari_crypto::ristretto::RistrettoSecretKey;
    use tari_test_utils::unpack_enum;
    use tari_utilities::ByteArray;

    use super::*;
    use crate::{
        mempool::{MempoolService, TxStorageResponse},
        proto::types::{AggregateBody, PrivateKey, Transaction},
    };

    #[tokio::test]
    async fn it_submits_transaction() {
        let (service, mempool, req_mock) = setup();
        let expected = TxStorageResponse::UnconfirmedPool;
        mempool.set_submit_transaction_response(expected.clone()).await;
        let txn = Transaction {
            offset: Some(PrivateKey {
                data: RistrettoSecretKey::default().to_vec(),
            }),
            body: Some(AggregateBody {
                inputs: vec![],
                outputs: vec![],
                kernels: vec![],
            }),
            script_offset: Some(PrivateKey {
                data: RistrettoSecretKey::default().to_vec(),
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

    #[tokio::test]
    async fn it_errors_on_invalid_transaction() {
        let (service, _, req_mock) = setup();
        let status = service
            .submit_transaction(req_mock.request_with_context(Default::default(), Default::default()))
            .await
            .unwrap_err();

        unpack_enum!(RpcStatusCode::BadRequest = status.as_status_code());
    }
}
