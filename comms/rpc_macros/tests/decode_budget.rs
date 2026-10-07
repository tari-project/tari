// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! End-to-end tests of the decode budget in generated RPC code: a real RPC server in front of a generated server
//! struct, and a generated client.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use futures::StreamExt;
use tari_comms::{
    PeerConnection,
    peer_manager::PeerFeatures,
    protocol::{
        ProtocolId,
        rpc::{
            NamedProtocolService,
            Request,
            Response,
            RpcError,
            RpcStatus,
            RpcStatusCode,
            Streaming,
            mock::MockRpcServer,
        },
    },
    test_utils::node_identity::build_node_identity,
};
use tari_comms_rpc_macros::tari_rpc;
use tari_test_utils::unpack_enum;
use tokio::sync::mpsc;

#[derive(Clone, PartialEq, prost::Message, tari_comms_rpc_macros::DecodeBudget)]
pub struct Empty {}

/// A message whose elements cost two bytes each on the wire but a whole struct each when decoded
#[derive(Clone, PartialEq, prost::Message, tari_comms_rpc_macros::DecodeBudget)]
pub struct Bomb {
    #[prost(message, repeated, tag = "1")]
    pub items: Vec<Empty>,
}

#[derive(Clone, PartialEq, prost::Message, tari_comms_rpc_macros::DecodeBudget)]
pub struct Item {
    #[prost(bytes, tag = "1")]
    pub data: Vec<u8>,
}

/// A message with many legitimate (non-empty) elements
#[derive(Clone, PartialEq, prost::Message, tari_comms_rpc_macros::DecodeBudget)]
pub struct Items {
    #[prost(message, repeated, tag = "1")]
    pub items: Vec<Item>,
}

#[tari_rpc(protocol_name = b"/test/decode-budget/1", server_struct = BudgetServer, client_struct = BudgetClient)]
pub trait Budget: Send + Sync + 'static {
    #[rpc(method = 1)]
    async fn take_bomb(&self, request: Request<Bomb>) -> Result<Response<()>, RpcStatus>;

    #[rpc(method = 2)]
    async fn take_items(&self, request: Request<Items>) -> Result<Response<()>, RpcStatus>;

    #[rpc(method = 3, max_items = 300_000)]
    async fn take_many_items(&self, request: Request<Items>) -> Result<Response<()>, RpcStatus>;

    #[rpc(method = 4)]
    async fn stream_bombs(&self, request: Request<()>) -> Result<Streaming<Bomb>, RpcStatus>;

    #[rpc(method = 5)]
    async fn reply_bomb(&self, request: Request<()>) -> Result<Response<Bomb>, RpcStatus>;

    #[rpc(method = 6, max_items = 300_000, max_request_items = 1_000)]
    async fn take_few_items(&self, request: Request<Items>) -> Result<Response<Items>, RpcStatus>;
}

#[derive(Clone, Default)]
struct BudgetService {
    calls: Arc<AtomicUsize>,
}

impl BudgetService {
    fn called(&self) {
        self.calls.fetch_add(1, Ordering::SeqCst);
    }
}

fn bomb(num_items: usize) -> Bomb {
    Bomb {
        items: vec![Empty {}; num_items],
    }
}

fn items(num_items: usize) -> Items {
    Items {
        items: vec![Item { data: vec![0xaa; 16] }; num_items],
    }
}

#[tari_comms::async_trait]
impl Budget for BudgetService {
    async fn take_bomb(&self, _: Request<Bomb>) -> Result<Response<()>, RpcStatus> {
        self.called();
        Ok(Response::new(()))
    }

    async fn take_items(&self, _: Request<Items>) -> Result<Response<()>, RpcStatus> {
        self.called();
        Ok(Response::new(()))
    }

    async fn take_many_items(&self, _: Request<Items>) -> Result<Response<()>, RpcStatus> {
        self.called();
        Ok(Response::new(()))
    }

    async fn stream_bombs(&self, _: Request<()>) -> Result<Streaming<Bomb>, RpcStatus> {
        self.called();
        let (tx, rx) = mpsc::channel(2);
        tx.send(Ok(bomb(10))).await.unwrap();
        tx.send(Ok(bomb(100_000))).await.unwrap();
        Ok(Streaming::new(rx))
    }

    async fn reply_bomb(&self, _: Request<()>) -> Result<Response<Bomb>, RpcStatus> {
        self.called();
        Ok(Response::new(bomb(100_000)))
    }

    async fn take_few_items(&self, _: Request<Items>) -> Result<Response<Items>, RpcStatus> {
        self.called();
        Ok(Response::new(items(100_000)))
    }
}

/// The server and connection stop serving when dropped, so tests hold on to them
type Guard = (MockRpcServer<BudgetServer<BudgetService>>, PeerConnection);

async fn setup() -> (BudgetClient, Arc<AtomicUsize>, Guard) {
    let service = BudgetService::default();
    let calls = service.calls.clone();
    let mut server = MockRpcServer::new(
        BudgetServer::new(service),
        build_node_identity(PeerFeatures::COMMUNICATION_NODE),
    );
    server.serve();
    let client_peer = build_node_identity(PeerFeatures::COMMUNICATION_NODE).to_peer();
    let mut conn = server
        .create_connection(client_peer, ProtocolId::from_static(BudgetClient::PROTOCOL_NAME))
        .await;
    let client = conn.connect_rpc::<BudgetClient>().await.unwrap();
    (client, calls, (server, conn))
}

#[tokio::test]
async fn the_server_rejects_a_bomb_before_the_handler_runs() {
    let (mut client, calls, _guard) = setup().await;

    let err = client.take_bomb(bomb(100_000)).await.unwrap_err();
    unpack_enum!(RpcError::RequestFailed(status) = err);
    assert_eq!(status.as_status_code(), RpcStatusCode::BadRequest);
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    // The session stays open and a legitimate request is served
    client.take_bomb(bomb(10)).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn the_server_applies_the_per_method_budget() {
    let (mut client, calls, _guard) = setup().await;

    // 100,000 legitimate elements: over the default budget, within the method's own
    let err = client.take_items(items(100_000)).await.unwrap_err();
    unpack_enum!(RpcError::RequestFailed(status) = err);
    assert_eq!(status.as_status_code(), RpcStatusCode::BadRequest);
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    client.take_many_items(items(100_000)).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn the_server_applies_the_request_budget() {
    let (mut client, calls, _guard) = setup().await;

    // Over max_request_items, though well within max_items
    let err = client.take_few_items(items(2_000)).await.unwrap_err();
    unpack_enum!(RpcError::RequestFailed(status) = err);
    assert_eq!(status.as_status_code(), RpcStatusCode::BadRequest);
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    // The response still gets the larger max_items
    let resp = client.take_few_items(items(500)).await.unwrap();
    assert_eq!(resp.items.len(), 100_000);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn the_client_rejects_a_bomb_stream_item() {
    let (mut client, _, _guard) = setup().await;

    let mut stream = client.stream_bombs().await.unwrap();
    let first = stream.next().await.unwrap().unwrap();
    assert_eq!(first.items.len(), 10);

    let err = stream.next().await.unwrap().unwrap_err();
    assert!(err.is_caused_by_server());
    unpack_enum!(RpcError::DecodeBudgetExceeded { .. } = err);
}

#[tokio::test]
async fn the_client_rejects_a_bomb_response() {
    let (mut client, _, _guard) = setup().await;

    let err = client.reply_bomb().await.unwrap_err();
    assert!(err.is_caused_by_server());
    unpack_enum!(RpcError::DecodeBudgetExceeded { .. } = err);
}
