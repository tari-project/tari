//  Copyright 2021, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::{
    framing,
    memsocket::MemorySocket,
    protocol::rpc::{
        error::HandshakeRejectReason,
        handshake::{RpcHandshakeError, SUPPORTED_RPC_VERSIONS},
        Handshake,
    },
    runtime,
};
use tari_test_utils::unpack_enum;
use tokio::task;

#[runtime::test_basic]
async fn it_performs_the_handshake() {
    let (client, server) = MemorySocket::new_pair();

    let handshake_result = task::spawn(async move {
        let mut server_framed = framing::canonical(server, 1024);
        let mut handshake_server = Handshake::new(&mut server_framed);
        handshake_server.perform_server_handshake().await
    });

    let mut client_framed = framing::canonical(client, 1024);
    let mut handshake_client = Handshake::new(&mut client_framed);

    handshake_client.perform_client_handshake().await.unwrap();
    let v = handshake_result.await.unwrap().unwrap();
    assert!(SUPPORTED_RPC_VERSIONS.contains(&v));
}

#[runtime::test_basic]
async fn it_rejects_the_handshake() {
    let (client, server) = MemorySocket::new_pair();

    let mut client_framed = framing::canonical(client, 1024);
    let mut handshake_client = Handshake::new(&mut client_framed);

    let mut server_framed = framing::canonical(server, 1024);
    let mut handshake_server = Handshake::new(&mut server_framed);
    handshake_server
        .reject_with_reason(HandshakeRejectReason::NoSessionsAvailable)
        .await
        .unwrap();

    let err = handshake_client.perform_client_handshake().await.unwrap_err();
    unpack_enum!(RpcHandshakeError::Rejected(reason) = err);
    unpack_enum!(HandshakeRejectReason::NoSessionsAvailable = reason);
}
