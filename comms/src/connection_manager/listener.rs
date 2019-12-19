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

use super::error::ConnectionManagerError;
use crate::{
    connection::ConnectionDirection,
    connection_manager::{next::ConnectionManagerEvent, peer_connection::create_peer_connection},
    multiaddr::Multiaddr,
    transports::Transport,
    types::CommsPublicKey,
};
use futures::{channel::mpsc, AsyncRead, AsyncWrite, SinkExt, StreamExt};
use log::*;
use tari_shutdown::ShutdownSignal;
use tokio::runtime::TaskExecutor;

const LOG_TARGET: &str = "comms::connection_manager::listener";

pub struct PeerListener<TTransport> {
    listen_address: Multiaddr,
    executor: TaskExecutor,
    conn_man_notifier: mpsc::Sender<ConnectionManagerEvent>,
    shutdown_signal: Option<ShutdownSignal>,
    transport: TTransport,
    transport_address: Option<Multiaddr>,
}

impl<TTransport, TSocket> PeerListener<TTransport>
where
    TTransport: Transport<Output = (TSocket, CommsPublicKey, Multiaddr)>,
    TSocket: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    pub fn new(
        executor: TaskExecutor,
        listen_address: Multiaddr,
        transport: TTransport,
        event_tx: mpsc::Sender<ConnectionManagerEvent>,
        shutdown_signal: ShutdownSignal,
    ) -> Self
    {
        Self {
            executor,
            listen_address,
            transport,
            conn_man_notifier: event_tx,
            shutdown_signal: Some(shutdown_signal),
            transport_address: None,
        }
    }

    pub async fn run(mut self) {
        let mut shutdown_signal = self
            .shutdown_signal
            .take()
            .expect("PeerListener initialized without a ShutdownSignal");

        match self.listen().await {
            Ok((inbound, address)) => {
                let mut inbound = inbound.fuse();
                futures::pin_mut!(inbound);
                self.transport_address = Some(address);

                loop {
                    futures::select! {
                        inbound_result = inbound.select_next_some() => {
                            if let Some(inbound_future) = log_if_error!(target: LOG_TARGET, inbound_result, "Inbound connection failed because '{error}'",) {
                                // TODO: Add inbound_future to FuturesUnordered stream to allow multiple peers to connect simultaneously
                                if let Some((socket, public_key, peer_addr)) = log_if_error!(target: LOG_TARGET, inbound_future.await,  "Inbound connection failed because '{error}'",) {
                                    self.handle_inbound_connection(socket, public_key, peer_addr).await;
                                }
                            }
                        },
                        _ = shutdown_signal => {
                            info!(target: LOG_TARGET, "PeerListener is shutting down because the shutdown signal was triggered");
                            break;
                        },
                    }
                }
            },
            Err(err) => {
                error!(target: LOG_TARGET, "PeerListener was unable to start because '{}'", err);
            },
        }
    }

    async fn handle_inbound_connection(
        &mut self,
        socket: TSocket,
        peer_public_key: CommsPublicKey,
        peer_addr: Multiaddr,
    )
    {
        match create_peer_connection(
            self.executor.clone(),
            socket,
            peer_addr,
            peer_public_key.clone(),
            ConnectionDirection::Inbound,
            self.conn_man_notifier.clone(),
        )
        .await
        {
            Ok(peer_conn) => {
                self.notify_connection_manager(ConnectionManagerEvent::PeerConnected(Box::new(peer_conn)))
                    .await;
            },
            Err(err) => {
                self.notify_connection_manager(ConnectionManagerEvent::PeerConnectFailed(
                    Box::new(peer_public_key),
                    err,
                ))
                .await
            },
        }
    }

    async fn listen(&self) -> Result<(TTransport::Listener, Multiaddr), ConnectionManagerError> {
        self.transport
            .listen(self.listen_address.clone())
            .await
            .map_err(|err| ConnectionManagerError::TransportError(err.to_string()))
    }

    pub fn transport_address(&self) -> Option<&Multiaddr> {
        self.transport_address.as_ref()
    }

    pub async fn notify_connection_manager(&mut self, event: ConnectionManagerEvent) {
        log_if_error!(
            target: LOG_TARGET,
            self.conn_man_notifier.send(event).await,
            "Failed to publish event because '{error}'",
        );
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn todo() {}
}
