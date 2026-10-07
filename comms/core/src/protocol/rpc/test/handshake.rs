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

use tari_test_utils::unpack_enum;
use tokio::task;

use crate::{
    framing,
    memsocket::MemorySocket,
    protocol::rpc::{
        Handshake,
        error::HandshakeRejectReason,
        handshake::{MAX_HANDSHAKE_FRAME_SIZE, RpcHandshakeError, SUPPORTED_RPC_VERSIONS},
    },
};

/// A frame one byte over the handshake limit: a valid `RpcSession`/`RpcSessionReply` padded with an unknown bytes field
fn oversized_handshake_frame() -> bytes::Bytes {
    let mut frame = Vec::new();
    prost::encoding::encode_key(15, prost::encoding::WireType::LengthDelimited, &mut frame);
    prost::encoding::encode_varint((MAX_HANDSHAKE_FRAME_SIZE - 2) as u64, &mut frame);
    frame.resize(MAX_HANDSHAKE_FRAME_SIZE + 1, 0);
    frame.into()
}

#[tokio::test]
async fn the_server_rejects_an_oversized_handshake_frame() {
    use futures::SinkExt;

    let (client, server) = MemorySocket::new_pair();
    let mut client_framed = framing::canonical(client, 4096);
    client_framed.send(oversized_handshake_frame()).await.unwrap();

    let mut server_framed = framing::canonical(server, 4096);
    let err = Handshake::new(&mut server_framed)
        .perform_server_handshake()
        .await
        .unwrap_err();
    unpack_enum!(RpcHandshakeError::FrameTooLarge { size, max } = err);
    assert_eq!(size, MAX_HANDSHAKE_FRAME_SIZE + 1);
    assert_eq!(max, MAX_HANDSHAKE_FRAME_SIZE);
}

#[tokio::test]
async fn the_client_rejects_an_oversized_handshake_reply() {
    use futures::SinkExt;

    let (client, server) = MemorySocket::new_pair();
    let mut server_framed = framing::canonical(server, 4096);
    server_framed.send(oversized_handshake_frame()).await.unwrap();

    let mut client_framed = framing::canonical(client, 4096);
    let err = Handshake::new(&mut client_framed)
        .perform_client_handshake()
        .await
        .unwrap_err();
    unpack_enum!(RpcHandshakeError::FrameTooLarge { .. } = err);
    // An honest server never sends an oversized reply, so the client blames the server for it
    assert!(crate::protocol::rpc::RpcError::from(err).is_caused_by_server());
}

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
