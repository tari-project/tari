// Copyright 2019 The Tari Project
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
    connection::PeerConnection,
    connection_manager::{dialer::Dialer, ConnectionManagerError},
    peer_manager::NodeId,
    shutdown::ShutdownSignal,
};
use futures::{
    channel::{mpsc, oneshot},
    future::{BoxFuture, FutureExt},
    sink::SinkExt,
    stream::{FusedStream, FuturesUnordered, StreamExt},
    Stream,
};
use log::*;
use std::sync::Arc;

const LOG_TARGET: &'static str = "comms::dialer::actor";

/// Create a connected ConnectionManagerRequester and ConnectionManagerService pair. The ConnectionManagerService
/// should be started using an executor `e.g. pool.spawn(service.start());`. The requester is used to
/// make requests to the started ConnectionManagerService.
pub fn create<TDialer>(
    buffer_size: usize,
    dialer: Arc<TDialer>,
    shutdown_signal: ShutdownSignal,
) -> (
    ConnectionManagerRequester,
    ConnectionManagerActor<TDialer, mpsc::Receiver<ConnectionManagerRequest>>,
)
{
    let (sender, receiver) = mpsc::channel(buffer_size);
    let actor = ConnectionManagerActor::new(dialer, receiver, shutdown_signal);
    let requester = ConnectionManagerRequester::new(sender);
    (requester, actor)
}

/// Requests which are handled by the ConnectionManagerService
pub enum ConnectionManagerRequest {
    DialPeer(
        Box<(
            NodeId,
            oneshot::Sender<Result<Arc<PeerConnection>, ConnectionManagerError>>,
        )>,
    ),
}

/// Responsible for constructing requests to the ConnectionManagerService
#[derive(Clone)]
pub struct ConnectionManagerRequester {
    sender: mpsc::Sender<ConnectionManagerRequest>,
}

impl ConnectionManagerRequester {
    /// Create a new ConnectionManagerRequester
    pub fn new(sender: mpsc::Sender<ConnectionManagerRequest>) -> Self {
        Self { sender }
    }
}

impl ConnectionManagerRequester {
    /// Attempt to connect to a remote peer
    pub async fn dial_node(&mut self, node_id: NodeId) -> Result<Arc<PeerConnection>, ConnectionManagerError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(ConnectionManagerRequest::DialPeer(Box::new((node_id, reply_tx))))
            .await
            .map_err(|_| ConnectionManagerError::SendToActorFailed)?;
        reply_rx
            .await
            .map_err(|_| ConnectionManagerError::ActorRequestCanceled)?
    }
}

/// # Connection Manager Actor
///
/// Responsible for executing connection requests.
pub struct ConnectionManagerActor<TDialer, TStream> {
    dialer: Arc<TDialer>,
    pending_dial_tasks: FuturesUnordered<BoxFuture<'static, ()>>,
    request_rx: TStream,
    shutdown_signal: Option<ShutdownSignal>,
}

impl<TDialer, TStream> ConnectionManagerActor<TDialer, TStream> {
    /// Create a new ConnectionManagerActor
    pub fn new(dialer: Arc<TDialer>, request_rx: TStream, shutdown_signal: ShutdownSignal) -> Self {
        Self {
            dialer,
            request_rx,
            pending_dial_tasks: FuturesUnordered::new(),
            shutdown_signal: Some(shutdown_signal),
        }
    }
}

impl<TDialer, TStream> ConnectionManagerActor<TDialer, TStream>
where
    TStream: Stream<Item = ConnectionManagerRequest> + FusedStream + Unpin,
    TDialer: Dialer<NodeId, Output = Arc<PeerConnection>, Error = ConnectionManagerError> + Send + Sync + 'static,
    TDialer::Future: Send,
{
    /// Start the connection manager actor
    pub async fn start(mut self) {
        let mut shutdown_signal = self
            .shutdown_signal
            .take()
            .expect("ConnectionManagerActor initialized without shutdown signal")
            .fuse();
        loop {
            ::futures::select! {
                // Handle requests to the ConnectionManagerActor
                request = self.request_rx.select_next_some() => { self.handle_request(request); },
                // Make progress pending connection tasks
                () = self.pending_dial_tasks.select_next_some() => { },
                _guard = shutdown_signal => {
                    info!(
                        target: LOG_TARGET,
                        "Shutting down connection manager actor because the shutdown signal was received",
                    );
                    break;
                },
                complete => {
                    info!(
                        target: LOG_TARGET,
                        "Shutting down connection manager actor because the request stream and all tasks completed",
                    );
                    break;
                },
            }
        }
    }

    fn handle_request(&mut self, request: ConnectionManagerRequest) {
        match request {
            ConnectionManagerRequest::DialPeer(boxed) => {
                let (node_id, reply_tx) = *boxed;
                let dialer = Arc::clone(&self.dialer);

                let connect_future = async move {
                    let result = dialer.dial(&node_id).await;

                    match reply_tx.send(result) {
                        Ok(_) => {},
                        Err(_msg) => {
                            error!(
                                target: LOG_TARGET,
                                "Unable to send connection result back to requester. Request was cancelled.",
                            );
                        },
                    }
                };

                self.pending_dial_tasks.push(connect_future.boxed());
            },
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_utils::dialers::CountDialer;
    use tokio::runtime::current_thread;

    #[test]
    fn requester_dial_peer() {
        let mut rt = current_thread::Runtime::new().unwrap();
        let (tx, mut rx) = mpsc::channel(1);
        let mut requester = ConnectionManagerRequester::new(tx);
        let node_id = NodeId::new();
        let node_id_clone = node_id.clone();

        let assert_request = async move {
            let msg = rx.next().await.unwrap();
            match msg {
                ConnectionManagerRequest::DialPeer(boxed) => {
                    let (req_node_id, reply_tx) = *boxed;
                    assert_eq!(req_node_id, node_id_clone);
                    drop(reply_tx);
                },
            }
        };
        rt.spawn(assert_request);

        match rt.block_on(requester.dial_node(node_id)) {
            Err(ConnectionManagerError::ActorRequestCanceled) => {},
            _ => panic!("unexpected result"),
        }
    }

    #[test]
    fn connection_manager_service_calls_dialer() {
        let mut rt = current_thread::Runtime::new().unwrap();

        let dialer = Arc::new(CountDialer::<NodeId>::new());
        let (_shutdown_tx, shutdown_rx) = oneshot::channel();
        let (mut requester, service) = create(1, Arc::clone(&dialer), shutdown_rx);

        rt.spawn(service.start());

        let node_id = NodeId::new();
        let _ = rt.block_on(requester.dial_node(node_id.clone())).unwrap();

        assert_eq!(dialer.count(), 1);
    }
}
