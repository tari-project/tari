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
    future::Future,
    time::{Duration, Instant},
};

use futures::{future, SinkExt, StreamExt};
use log::*;
use multiaddr::Multiaddr;
use tari_shutdown::ShutdownSignal;
use tokio::{
    io::{AsyncRead, AsyncWrite, AsyncWriteExt},
    sync::watch,
    time,
};
use tokio_util::codec::{Framed, LinesCodec, LinesCodecError};

use crate::{connection_manager::wire_mode::WireMode, transports::Transport};

/// Max line length accepted by the liveness session.
const MAX_LINE_LENGTH: usize = 50;
const LOG_TARGET: &str = "comms::connection_manager::liveness";

/// Echo server for liveness checks
pub struct LivenessSession<TSocket> {
    framed: Framed<TSocket, LinesCodec>,
}

impl<TSocket> LivenessSession<TSocket>
where TSocket: AsyncRead + AsyncWrite + Unpin
{
    pub fn new(socket: TSocket) -> Self {
        Self {
            framed: Framed::new(socket, LinesCodec::new_with_max_length(MAX_LINE_LENGTH)),
        }
    }

    pub fn run(self) -> impl Future<Output = Result<(), LinesCodecError>> {
        let (sink, stream) = self.framed.split();
        stream.forward(sink)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum LivenessStatus {
    Disabled,
    Checking,
    Unreachable,
    Live(Duration),
}

pub struct LivenessCheck<TTransport> {
    transport: TTransport,
    addresses: Vec<Multiaddr>,
    interval: Duration,
    tx_watch: watch::Sender<LivenessStatus>,
    shutdown_signal: ShutdownSignal,
}

impl<TTransport> LivenessCheck<TTransport>
where
    TTransport: Transport + Send + Sync + 'static,
    TTransport::Output: AsyncRead + AsyncWrite + Unpin + Send,
{
    pub fn spawn(
        transport: TTransport,
        addresses: Vec<Multiaddr>,
        interval: Duration,
        shutdown_signal: ShutdownSignal,
    ) -> watch::Receiver<LivenessStatus> {
        let (tx_watch, rx_watch) = watch::channel(LivenessStatus::Checking);
        let check = Self {
            transport,
            addresses,
            interval,
            tx_watch,
            shutdown_signal,
        };
        tokio::spawn(check.run_until_shutdown());
        rx_watch
    }

    pub async fn run_until_shutdown(self) {
        let shutdown_signal = self.shutdown_signal.clone();
        let run_fut = self.run();
        tokio::pin!(run_fut);
        future::select(run_fut, shutdown_signal).await;
    }

    pub async fn run(mut self) {
        info!(
            target: LOG_TARGET,
            "🔌️ Starting liveness self-check with interval {:.2?}", self.interval
        );
        let mut address_idx = 0;
        let mut last_working_address = None;
        loop {
            let timer = Instant::now();
            let _ = self.tx_watch.send(LivenessStatus::Checking);
            let address = last_working_address
                .clone()
                .unwrap_or(self.addresses[address_idx].clone());
            match self.transport.dial(&address).await {
                Ok(mut socket) => {
                    debug!(
                        target: LOG_TARGET,
                        "🔌 liveness dial ({}) took {:.2?}",
                        address,
                        timer.elapsed()
                    );
                    if let Err(err) = socket.write(&[WireMode::Liveness.as_byte()]).await {
                        warn!(target: LOG_TARGET, "🔌️ liveness failed to write byte: {}", err);
                        self.tx_watch.send_replace(LivenessStatus::Unreachable);
                        continue;
                    }
                    last_working_address = Some(address);
                    let mut framed = Framed::new(socket, LinesCodec::new_with_max_length(MAX_LINE_LENGTH));
                    loop {
                        match self.ping_pong(&mut framed).await {
                            Ok(Some(latency)) => {
                                debug!(target: LOG_TARGET, "⚡️️ liveness check latency {:.2?}", latency);
                                self.tx_watch.send_replace(LivenessStatus::Live(latency));
                            },
                            Ok(None) => {
                                info!(target: LOG_TARGET, "🔌️ liveness connection closed");
                                self.tx_watch.send_replace(LivenessStatus::Unreachable);
                                break;
                            },
                            Err(err) => {
                                warn!(target: LOG_TARGET, "🔌️ ping pong failed: {}", err);
                                self.tx_watch.send_replace(LivenessStatus::Unreachable);
                                // let _ = framed.close().await;
                                break;
                            },
                        }

                        time::sleep(self.interval).await;
                    }
                },
                Err(err) => {
                    address_idx = (address_idx + 1) % self.addresses.len();
                    self.tx_watch.send_replace(LivenessStatus::Unreachable);
                    warn!(
                        target: LOG_TARGET,
                        "🔌️ Failed to dial public address {} for self check: {}", address, err
                    );
                },
            }
            time::sleep(self.interval).await;
        }
    }

    async fn ping_pong(
        &mut self,
        framed: &mut Framed<TTransport::Output, LinesCodec>,
    ) -> Result<Option<Duration>, LinesCodecError> {
        let timer = Instant::now();
        framed.send("pingpong".to_string()).await?;
        match framed.next().await {
            Some(res) => {
                let val = res?;
                debug!(target: LOG_TARGET, "Received: {}", val);
                Ok(Some(timer.elapsed()))
            },
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod test {
    use futures::SinkExt;
    use tokio::{time, time::Duration};
    use tokio_stream::StreamExt;

    use super::*;
    use crate::memsocket::MemorySocket;

    #[tokio::test]
    async fn echos() {
        let (inbound, outbound) = MemorySocket::new_pair();
        let liveness = LivenessSession::new(inbound);
        let join_handle = tokio::spawn(liveness.run());
        let mut outbound = Framed::new(outbound, LinesCodec::new());
        for _ in 0..10usize {
            outbound.send("ECHO".to_string()).await.unwrap()
        }
        let pings = outbound.take(10).collect::<Vec<_>>().await;
        assert_eq!(pings.len(), 10);
        assert!(pings.iter().all(|a| a.as_ref().unwrap() == "ECHO"));

        time::timeout(Duration::from_secs(1), join_handle)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}
