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

use super::Transport;
use crate::{
    connection::ConnectionDirection,
    noise::{NoiseConfig, NoiseSocket},
    types::CommsPublicKey,
};
use futures::{AsyncRead, AsyncWrite, Future, Stream, StreamExt};
use multiaddr::Multiaddr;
use std::{io, marker::PhantomData};

/// Transport implementation for TCP
#[derive(Debug)]
pub struct NoiseTransport<TTransport, TSocket> {
    transport: TTransport,
    noise_config: NoiseConfig,
    _socket: PhantomData<TSocket>,
}

impl<TTransport, TSocket> NoiseTransport<TTransport, TSocket> {
    /// Create a new TcpNoiseTransport
    pub fn new(transport: TTransport, noise_config: NoiseConfig) -> Self {
        Self {
            transport,
            noise_config,
            _socket: PhantomData,
        }
    }
}
impl<TSocket, TTransport> Clone for NoiseTransport<TTransport, TSocket>
where TTransport: Clone
{
    fn clone(&self) -> Self {
        Self {
            transport: self.transport.clone(),
            noise_config: self.noise_config.clone(),
            _socket: PhantomData,
        }
    }
}

impl<TSocket, TTransport> Transport for NoiseTransport<TTransport, TSocket>
where
    TTransport: Transport<Output = (TSocket, Multiaddr)> + Send + Sync + Clone,
    TTransport::Error: From<io::Error>,
    TSocket: AsyncRead + AsyncWrite + Send + Sync + Unpin,
{
    type Error = TTransport::Error;
    type Output = (NoiseSocket<TSocket>, CommsPublicKey, Multiaddr);

    type DialFuture = impl Future<Output = Result<Self::Output, Self::Error>>;
    type Inbound = impl Future<Output = Result<Self::Output, Self::Error>>;
    type ListenFuture = impl Future<Output = Result<(Self::Listener, Multiaddr), Self::Error>>;
    type Listener = impl Stream<Item = Result<Self::Inbound, Self::Error>>;

    fn listen(&self, addr: Multiaddr) -> Self::ListenFuture {
        let noise_config = self.noise_config.clone();
        let transport = self.transport.clone();
        Box::pin(async move {
            let (listener, listen_address) = transport.listen(addr).await?;
            Ok((
                listener.map(move |inbound_result| {
                    let noise_config_clone = noise_config.clone();
                    // Create a future which does the upgrade and return it on the stream
                    let fut = async move {
                        let (socket, peer_addr) = inbound_result?.await?;
                        match noise_config_clone
                            .upgrade_socket(socket, ConnectionDirection::Inbound)
                            .await
                        {
                            Ok((public_key, noise_socket)) => Ok((noise_socket, public_key, peer_addr)),
                            Err(err) => Err(io::Error::new(io::ErrorKind::Other, err).into()),
                        }
                    };

                    Ok(fut)
                }),
                listen_address,
            ))
        })
    }

    fn dial(&self, addr: Multiaddr) -> Self::DialFuture {
        let noise_config = self.noise_config.clone();
        let transport = self.transport.clone();
        Box::pin(async move {
            let (socket, peer_addr) = transport.dial(addr).await?;
            let (public_key, socket) = noise_config
                .upgrade_socket(socket, ConnectionDirection::Outbound)
                .await
                .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;

            Ok((socket, public_key, peer_addr))
        })
    }
}
