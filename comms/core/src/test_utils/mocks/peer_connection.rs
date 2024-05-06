// Copyright 2020, The Tari Project
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

use std::{
    str::FromStr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use tokio::{
    runtime::Handle,
    sync::{mpsc, Mutex},
};
use tokio_stream::StreamExt;
use yamux::ConnectionError;

use crate::{
    connection_manager::{
        ConnectionDirection,
        NegotiatedSubstream,
        PeerConnection,
        PeerConnectionError,
        PeerConnectionRequest,
    },
    multiaddr::Multiaddr,
    multiplexing,
    multiplexing::{IncomingSubstreams, Substream, Yamux},
    peer_manager::{NodeId, Peer, PeerFeatures},
    test_utils::{node_identity::build_node_identity, transport},
    utils::atomic_ref_counter::AtomicRefCounter,
};

static ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub fn create_dummy_peer_connection(node_id: NodeId) -> (PeerConnection, mpsc::Receiver<PeerConnectionRequest>) {
    let (tx, rx) = mpsc::channel(1);
    let addr = Multiaddr::from_str("/ip4/23.23.23.23/tcp/80").unwrap();
    (
        PeerConnection::new(
            1,
            tx,
            node_id,
            PeerFeatures::COMMUNICATION_NODE,
            addr,
            ConnectionDirection::Inbound,
            AtomicRefCounter::new(),
        ),
        rx,
    )
}

pub async fn create_peer_connection_mock_pair(
    peer1: Peer,
    peer2: Peer,
) -> (
    PeerConnection,
    PeerConnectionMockState,
    PeerConnection,
    PeerConnectionMockState,
) {
    let rt_handle = Handle::current();
    let (tx1, rx1) = mpsc::channel(1);
    let (tx2, rx2) = mpsc::channel(1);
    let (listen_addr, muxer_in, muxer_out) = transport::build_multiplexed_connections().await;

    // Start both mocks on current handle
    let mock = PeerConnectionMock::new(rx1, muxer_in);
    let mock_state_in = mock.get_shared_state();
    rt_handle.spawn(mock.run());
    let mock = PeerConnectionMock::new(rx2, muxer_out);
    let mock_state_out = mock.get_shared_state();
    rt_handle.spawn(mock.run());

    (
        PeerConnection::new(
            // ID must be unique since it is used for connection equivalency, so we re-implement this in the mock
            ID_COUNTER.fetch_add(1, Ordering::SeqCst),
            tx1,
            peer2.node_id,
            peer2.features,
            listen_addr.clone(),
            ConnectionDirection::Inbound,
            mock_state_in.substream_counter(),
        ),
        mock_state_in,
        PeerConnection::new(
            ID_COUNTER.fetch_add(1, Ordering::SeqCst),
            tx2,
            peer1.node_id,
            peer1.features,
            listen_addr,
            ConnectionDirection::Outbound,
            mock_state_out.substream_counter(),
        ),
        mock_state_out,
    )
}

pub async fn new_peer_connection_mock_pair() -> (
    PeerConnection,
    PeerConnectionMockState,
    PeerConnection,
    PeerConnectionMockState,
) {
    let peer1 = build_node_identity(PeerFeatures::COMMUNICATION_NODE).to_peer();
    let peer2 = build_node_identity(PeerFeatures::COMMUNICATION_NODE).to_peer();
    create_peer_connection_mock_pair(peer1, peer2).await
}

#[derive(Clone)]
pub struct PeerConnectionMockState {
    call_count: Arc<AtomicUsize>,
    mux_control: Arc<Mutex<multiplexing::Control>>,
    mux_incoming: Arc<Mutex<IncomingSubstreams>>,
    substream_counter: AtomicRefCounter,
}

impl PeerConnectionMockState {
    pub fn new(muxer: Yamux) -> Self {
        let control = muxer.get_yamux_control();
        let substream_counter = muxer.substream_counter();
        Self {
            call_count: Arc::new(AtomicUsize::new(0)),
            mux_control: Arc::new(Mutex::new(control)),
            mux_incoming: Arc::new(Mutex::new(muxer.into_incoming())),
            substream_counter,
        }
    }

    pub fn inc_call_count(&self) {
        self.call_count.fetch_add(1, Ordering::SeqCst);
    }

    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }

    pub async fn open_substream(&self) -> Result<Substream, PeerConnectionError> {
        self.mux_control.lock().await.open_stream().await.map_err(Into::into)
    }

    pub fn substream_counter(&self) -> AtomicRefCounter {
        self.substream_counter.clone()
    }

    pub fn num_open_substreams(&self) -> usize {
        self.substream_counter.get()
    }

    pub async fn next_incoming_substream(&self) -> Option<Substream> {
        self.mux_incoming.lock().await.next().await
    }

    pub async fn disconnect(&self) -> Result<(), PeerConnectionError> {
        match self.mux_control.lock().await.close().await {
            Ok(_) => Ok(()),
            // Match the behaviour of the real PeerConnection.
            Err(ConnectionError::Closed) => Ok(()),
            Err(err) => Err(err.into()),
        }
    }
}

pub struct PeerConnectionMock {
    receiver: mpsc::Receiver<PeerConnectionRequest>,
    state: PeerConnectionMockState,
}

impl PeerConnectionMock {
    pub fn new(receiver: mpsc::Receiver<PeerConnectionRequest>, muxer: Yamux) -> Self {
        Self {
            receiver,
            state: PeerConnectionMockState::new(muxer),
        }
    }

    pub fn get_shared_state(&self) -> PeerConnectionMockState {
        self.state.clone()
    }

    pub async fn run(mut self) {
        while let Some(req) = self.receiver.recv().await {
            self.handle_request(req).await;
        }
    }

    async fn handle_request(&mut self, req: PeerConnectionRequest) {
        use PeerConnectionRequest::{Disconnect, OpenSubstream};
        self.state.inc_call_count();
        match req {
            OpenSubstream { protocol_id, reply_tx } => match self.state.open_substream().await {
                Ok(stream) => {
                    let negotiated_substream = NegotiatedSubstream {
                        protocol: protocol_id,
                        stream,
                    };
                    reply_tx.send(Ok(negotiated_substream)).unwrap();
                },
                Err(err) => {
                    reply_tx.send(Err(err)).unwrap();
                },
            },
            Disconnect(_, reply_tx) => {
                self.receiver.close();
                reply_tx.send(self.state.disconnect().await).unwrap();
            },
        }
    }
}
