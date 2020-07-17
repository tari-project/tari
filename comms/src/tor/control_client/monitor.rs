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

use super::{event::TorControlEvent, parsers, response::ResponseLine, LOG_TARGET};
use crate::{compat::IoCompat, runtime::task};
use futures::{channel::mpsc, future, future::Either, AsyncRead, AsyncWrite, SinkExt, Stream, StreamExt};
use log::*;
use std::fmt;
use tokio::sync::broadcast;
use tokio_util::codec::{Framed, LinesCodec};

pub fn spawn_monitor<TSocket>(
    mut cmd_rx: mpsc::Receiver<String>,
    socket: TSocket,
    event_tx: broadcast::Sender<TorControlEvent>,
) -> mpsc::Receiver<ResponseLine>
where
    TSocket: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (mut responses_tx, responses_rx) = mpsc::channel(100);

    task::spawn(async move {
        let framed = Framed::new(IoCompat::new(socket), LinesCodec::new());
        let (mut sink, mut stream) = framed.split();
        loop {
            let either = future::select(cmd_rx.next(), stream.next()).await;
            match either {
                // Received a command to send to the control server
                Either::Left((Some(line), _)) => {
                    trace!(target: LOG_TARGET, "Writing command of length '{}'", line.len());
                    if let Err(err) = sink.send(line).await {
                        error!(
                            target: LOG_TARGET,
                            "Error when sending to Tor control server: {:?}. Monitor is shutting down.", err
                        );
                        break;
                    }
                },
                // Command stream ended
                Either::Left((None, _)) => {
                    debug!(
                        target: LOG_TARGET,
                        "Tor control server command receiver closed. Monitor is exiting."
                    );
                    break;
                },

                // Received a line from the control server
                Either::Right((Some(Ok(line)), _)) => {
                    trace!(target: LOG_TARGET, "Read line of length '{}'", line.len());
                    match parsers::response_line(&line) {
                        Ok(mut line) => {
                            if line.is_multiline {
                                let lines = read_until(&mut stream, ".").await;
                                line.value = format!("{}\n{}", line.value, lines.join("\n"));
                            }

                            if line.is_event() {
                                match TorControlEvent::try_from_response(line) {
                                    Ok(event) => {
                                        let _ = event_tx.send(event);
                                    },
                                    Err(err) => {
                                        log_server_response_error(err);
                                    },
                                }
                            } else if let Err(err) = responses_tx.send(line).await {
                                warn!(
                                    target: LOG_TARGET,
                                    "Failed to send response on internal channel: {:?}", err
                                );
                            }
                        },
                        Err(err) => log_server_response_error(err),
                    }
                },

                // Error receiving a line from the control server
                Either::Right((Some(Err(err)), _)) => {
                    error!(
                        target: LOG_TARGET,
                        "Line framing error when reading from tor control server: '{:?}'. Monitor is exiting.", err
                    );
                    break;
                },
                // The control server disconnected
                Either::Right((None, _)) => {
                    cmd_rx.close();
                    debug!(
                        target: LOG_TARGET,
                        "Connection to tor control port closed. Monitor is exiting."
                    );
                    let _ = event_tx.send(TorControlEvent::TorControlDisconnected);
                    break;
                },
            }
        }
    });

    responses_rx
}

async fn read_until<E: fmt::Debug, S: Stream<Item = Result<String, E>> + Unpin>(
    stream: &mut S,
    pat: &str,
) -> Vec<String>
{
    let mut items = Vec::new();
    loop {
        match stream.next().await {
            Some(Ok(item)) => {
                if item.trim() == pat {
                    break items;
                }
                items.push(item);
            },
            Some(Err(err)) => {
                error!(target: LOG_TARGET, "read_until: {:?}", err);
            },
            None => {
                break items;
            },
        }
    }
}

fn log_server_response_error<E: fmt::Debug>(err: E) {
    error!(
        target: LOG_TARGET,
        "Error processing response from tor control server: '{:?}'", err
    );
}
