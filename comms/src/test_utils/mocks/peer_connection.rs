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

use crate::{
    connection_manager::{
        ConnectionDirection,
        NegotiatedSubstream,
        PeerConnection,
        PeerConnectionError,
        PeerConnectionRequest,
    },
    multiplexing,
    multiplexing::{IncomingSubstreams, Yamux},
    peer_manager::NodeId,
    test_utils::transport,
};
use futures::{channel::mpsc, lock::Mutex, stream::Fuse, StreamExt};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::runtime::Handle;

pub async fn create_peer_connection_mock_pair(
    buf_size: usize,
    node_id_in: NodeId,
    node_id_out: NodeId,
) -> (
    PeerConnection,
    PeerConnectionMockState,
    PeerConnection,
    PeerConnectionMockState,
)
{
    let rt_handle = Handle::current();
    let (tx1, rx1) = mpsc::channel(buf_size);
    let (tx2, rx2) = mpsc::channel(buf_size);
    let (listen_addr, muxer_in, muxer_out) = transport::build_multiplexed_connections().await;

    // Start both mocks on current handle
    let mock = PeerConnectionMock::new(rx1.fuse(), muxer_in);
    let mock_state_in = mock.get_shared_state();
    rt_handle.spawn(mock.run());
    let mock = PeerConnectionMock::new(rx2.fuse(), muxer_out);
    let mock_state_out = mock.get_shared_state();
    rt_handle.spawn(mock.run());

    (
        PeerConnection::new(1, tx1, node_id_in, listen_addr.clone(), ConnectionDirection::Inbound),
        mock_state_in,
        PeerConnection::new(2, tx2, node_id_out, listen_addr, ConnectionDirection::Outbound),
        mock_state_out,
    )
}

#[derive(Debug, Clone)]
pub struct PeerConnectionMockState {
    call_count: Arc<AtomicUsize>,
    mux_control: Arc<Mutex<multiplexing::Control>>,
    mux_incoming: Arc<Mutex<IncomingSubstreams>>,
}

impl PeerConnectionMockState {
    pub fn new(muxer: Yamux) -> Self {
        Self {
            call_count: Arc::new(AtomicUsize::new(0)),
            mux_control: Arc::new(Mutex::new(muxer.get_yamux_control())),
            mux_incoming: Arc::new(Mutex::new(muxer.incoming())),
        }
    }

    pub fn inc_call_count(&self) {
        self.call_count.fetch_add(1, Ordering::SeqCst);
    }

    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }

    pub async fn open_substream(&self) -> Result<yamux::Stream, PeerConnectionError> {
        self.mux_control.lock().await.open_stream().await.map_err(Into::into)
    }

    pub async fn next_incoming_substream(&self) -> Option<yamux::Stream> {
        self.mux_incoming.lock().await.next().await
    }

    pub async fn disconnect(&self) {
        self.mux_control.lock().await.close().await.unwrap();
    }
}

pub struct PeerConnectionMock {
    receiver: Fuse<mpsc::Receiver<PeerConnectionRequest>>,
    state: PeerConnectionMockState,
}

impl PeerConnectionMock {
    pub fn new(receiver: Fuse<mpsc::Receiver<PeerConnectionRequest>>, muxer: Yamux) -> Self {
        Self {
            receiver,
            state: PeerConnectionMockState::new(muxer),
        }
    }

    pub fn get_shared_state(&self) -> PeerConnectionMockState {
        self.state.clone()
    }

    pub async fn run(mut self) {
        while let Some(req) = self.receiver.next().await {
            self.handle_request(req).await;
        }
    }

    async fn handle_request(&mut self, req: PeerConnectionRequest) {
        use PeerConnectionRequest::*;
        self.state.inc_call_count();
        match req {
            OpenSubstream(protocol, reply_tx) => match self.state.open_substream().await {
                Ok(stream) => {
                    let negotiated_substream = NegotiatedSubstream { protocol, stream };
                    reply_tx.send(Ok(negotiated_substream)).unwrap();
                },
                Err(err) => {
                    reply_tx.send(Err(err.into())).unwrap();
                },
            },
            Disconnect(_, reply_tx) => {
                self.state.disconnect().await;
                reply_tx.send(()).unwrap();
            },
        }
    }
}
