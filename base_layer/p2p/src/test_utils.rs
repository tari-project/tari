// Copyright 2019, The Tari Project
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

use futures::{future, task::Context, Future, Poll};
use rand::rngs::OsRng;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
    Mutex,
};
use tari_comms::{
    connection::NetAddress,
    message::{InboundMessage, MessageEnvelopeHeader, MessageFlags},
    peer_manager::{NodeIdentity, Peer, PeerFlags, PeerManager},
    types::CommsDatabase,
    utils::signature,
};
use tari_comms_dht::{
    inbound::DhtInboundMessage,
    message::{DhtEnvelope, DhtHeader, DhtMessageFlags, DhtMessageType, NodeDestination},
};
use tari_storage::lmdb_store::LMDBBuilder;
use tari_test_utils::{paths::create_random_database_path, random};
use tari_utilities::message_format::MessageFormat;
use tower::Service;

pub fn make_node_identity() -> Arc<NodeIdentity> {
    Arc::new(NodeIdentity::random(&mut OsRng::new().unwrap(), "127.0.0.1:9000".parse().unwrap()).unwrap())
}

pub fn make_comms_inbound_message(
    node_identity: &NodeIdentity,
    message: Vec<u8>,
    flags: MessageFlags,
) -> InboundMessage
{
    InboundMessage::new(
        Peer::new(
            node_identity.identity.public_key.clone(),
            node_identity.identity.node_id.clone(),
            Vec::<NetAddress>::new().into(),
            PeerFlags::empty(),
        ),
        MessageEnvelopeHeader {
            version: 0,
            message_public_key: node_identity.identity.public_key.clone(),
            message_signature: Vec::new(),
            flags,
        },
        0,
        message,
    )
}

pub fn make_dht_header(node_identity: &NodeIdentity, message: &Vec<u8>, flags: DhtMessageFlags) -> DhtHeader {
    DhtHeader {
        version: 0,
        destination: NodeDestination::Undisclosed,
        origin_public_key: node_identity.identity.public_key.clone(),
        origin_signature: signature::sign(&mut OsRng::new().unwrap(), node_identity.secret_key.clone(), message)
            .unwrap()
            .to_binary()
            .unwrap(),
        message_type: DhtMessageType::None,
        flags,
    }
}

pub fn make_dht_inbound_message(
    node_identity: &NodeIdentity,
    message: Vec<u8>,
    flags: DhtMessageFlags,
) -> DhtInboundMessage
{
    DhtInboundMessage::new(
        make_dht_header(node_identity, &message, flags),
        Peer::new(
            node_identity.identity.public_key.clone(),
            node_identity.identity.node_id.clone(),
            Vec::<NetAddress>::new().into(),
            PeerFlags::empty(),
        ),
        MessageEnvelopeHeader {
            version: 0,
            message_public_key: node_identity.identity.public_key.clone(),
            message_signature: Vec::new(),
            flags: MessageFlags::empty(),
        },
        message,
    )
}

pub fn make_dht_envelope(node_identity: &NodeIdentity, message: Vec<u8>, flags: DhtMessageFlags) -> DhtEnvelope {
    DhtEnvelope::new(make_dht_header(node_identity, &message, flags), message)
}

pub fn make_peer_manager() -> Arc<PeerManager> {
    let database_name = random::string(8);
    let path = create_random_database_path();
    let datastore = LMDBBuilder::new()
        .set_path(path.to_str().unwrap())
        .set_environment_size(10)
        .set_max_number_of_databases(1)
        .add_database(&database_name, lmdb_zero::db::CREATE)
        .build()
        .unwrap();

    let peer_database = datastore.get_handle(&database_name).unwrap();

    PeerManager::new(CommsDatabase::new(Arc::new(peer_database)))
        .map(Arc::new)
        .unwrap()
}

pub fn service_spy<TReq>() -> ServiceSpy<TReq>
where TReq: 'static {
    ServiceSpy::new()
}

#[derive(Clone)]
pub struct ServiceSpy<TReq> {
    requests: Arc<Mutex<Vec<TReq>>>,
    call_count: Arc<AtomicUsize>,
}

impl<TReq> ServiceSpy<TReq>
where TReq: 'static
{
    pub fn new() -> Self {
        let requests = Arc::new(Mutex::new(Vec::new()));
        Self {
            requests,
            call_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    #[allow(dead_code)]
    pub fn reset(&self) {
        self.call_count.store(0, Ordering::SeqCst);
        self.requests.lock().unwrap().clear();
    }

    pub fn service<TErr>(&self) -> impl Service<TReq, Response = (), Error = TErr> + Clone {
        let req_inner = Arc::clone(&self.requests);
        let call_count = Arc::clone(&self.call_count);
        service_fn(move |req: TReq| {
            req_inner.lock().unwrap().push(req);
            call_count.fetch_add(1, Ordering::SeqCst);
            future::ready(Result::<_, TErr>::Ok(()))
        })
    }

    #[allow(dead_code)]
    pub fn take_requests(&self) -> Vec<TReq> {
        self.requests.lock().unwrap().drain(..).collect()
    }

    pub fn pop_request(&self) -> Option<TReq> {
        self.requests.lock().unwrap().pop()
    }

    pub fn is_called(&self) -> bool {
        self.call_count() > 0
    }

    #[allow(dead_code)]
    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

//---------------------------------- ServiceFn --------------------------------------------//

// TODO: Remove this when https://github.com/tower-rs/tower/pull/318 is published

/// Returns a new `ServiceFn` with the given closure.
pub fn service_fn<T>(f: T) -> ServiceFn<T> {
    ServiceFn { f }
}

/// A `Service` implemented by a closure.
#[derive(Copy, Clone, Debug)]
pub struct ServiceFn<T> {
    f: T,
}

impl<T, F, Request, R, E> Service<Request> for ServiceFn<T>
where
    T: FnMut(Request) -> F,
    F: Future<Output = Result<R, E>>,
{
    type Error = E;
    type Future = F;
    type Response = R;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), E>> {
        Ok(()).into()
    }

    fn call(&mut self, req: Request) -> Self::Future {
        (self.f)(req)
    }
}
