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

use futures::{SinkExt, StreamExt};
use prost::Message;
use tari_test_utils::unpack_enum;
use tokio::task;

use crate::{
    framing,
    memsocket::MemorySocket,
    message::MessageExt,
    proto,
    protocol::rpc::{
        Handshake,
        error::HandshakeRejectReason,
        handshake::{MAX_SUPPORTED_RPC_VERSIONS, RpcHandshakeError, SUPPORTED_RPC_VERSIONS},
    },
};

#[tokio::test]
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

#[tokio::test]
async fn it_rejects_the_handshake() {
    let (client, server) = MemorySocket::new_pair();

    let mut client_framed = framing::canonical(client, 1024);
    let mut handshake_client = Handshake::new(&mut client_framed);

    let mut server_framed = framing::canonical(server, 1024);
    let mut handshake_server = Handshake::new(&mut server_framed);
    handshake_server
        .reject_with_reason(HandshakeRejectReason::NoServerSessionsAvailable("some reason"))
        .await
        .unwrap();

    let err = handshake_client.perform_client_handshake().await.unwrap_err();
    unpack_enum!(RpcHandshakeError::Rejected(reason) = err);
    unpack_enum!(HandshakeRejectReason::NoServerSessionsAvailable("session limit reached") = reason);
}

#[tokio::test]
async fn it_rejects_an_oversized_supported_versions_list() {
    let (client, server) = MemorySocket::new_pair();

    let handshake_result = task::spawn(async move {
        let mut server_framed = framing::canonical(server, 1024);
        let mut handshake_server = Handshake::new(&mut server_framed);
        handshake_server.perform_server_handshake().await
    });

    let mut client_framed = framing::canonical(client, 1024);
    let session = proto::rpc::RpcSession {
        supported_versions: vec![0; MAX_SUPPORTED_RPC_VERSIONS + 1],
    };
    client_framed.send(session.to_encoded_bytes().into()).await.unwrap();

    let reply = client_framed.next().await.unwrap().unwrap();
    let reply = proto::rpc::RpcSessionReply::decode(reply.freeze()).unwrap();
    let err = reply.result().unwrap_err();
    unpack_enum!(RpcHandshakeError::Rejected(reason) = err);
    unpack_enum!(HandshakeRejectReason::UnsupportedVersion = reason);

    unpack_enum!(RpcHandshakeError::ClientNoSupportedVersion = handshake_result.await.unwrap().unwrap_err());
}
