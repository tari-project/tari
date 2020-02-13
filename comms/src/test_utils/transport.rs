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
    connection::ConnectionDirection,
    memsocket::MemorySocket,
    multiaddr::Multiaddr,
    multiplexing::Yamux,
    transports::{MemoryTransport, Transport},
};
use futures::{future, StreamExt};
use tokio::runtime::Handle;

pub async fn build_connected_sockets() -> (Multiaddr, MemorySocket, MemorySocket) {
    let (mut listener, addr) = MemoryTransport
        .listen("/memory/0".parse().unwrap())
        .unwrap()
        .await
        .unwrap();
    let (dial_sock, listen_sock) = future::join(MemoryTransport.dial(addr.clone()).unwrap(), listener.next()).await;
    (addr, dial_sock.unwrap(), listen_sock.unwrap().unwrap().0.await.unwrap())
}

pub async fn build_multiplexed_connections() -> (Multiaddr, Yamux, Yamux) {
    let rt_handle = Handle::current();
    let (addr, socket_out, socket_in) = build_connected_sockets().await;

    let muxer_out = Yamux::upgrade_connection(rt_handle.clone(), socket_out, ConnectionDirection::Outbound)
        .await
        .unwrap();

    let muxer_in = Yamux::upgrade_connection(rt_handle, socket_in, ConnectionDirection::Inbound)
        .await
        .unwrap();

    (addr, muxer_out, muxer_in)
}
