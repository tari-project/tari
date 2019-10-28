//  Copyright 2019 The Tari Project
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

use derive_error::Error;

///// Control service request message types
//#[derive(Eq, PartialEq, Hash, Serialize, Deserialize)]
// pub enum ControlServiceRequestType {
//    RequestPeerConnection,
//    Ping,
//}
//
///// Control service response message types
//#[derive(Eq, PartialEq, Hash, Serialize, Deserialize)]
// pub enum ControlServiceResponseType {
//    AcceptPeerConnection,
//    RejectPeerConnection,
//    Pong,
//    ConnectRequestOutcome,
//}

///// Details required to connect to the new [PeerConnection]
/////
///// [PeerConnection]: ../../connection/peer_connection/index.html
//#[derive(Serialize, Deserialize, Debug)]
// pub struct PeerConnectionDetails {
//    pub server_key: CurvePublicKey,
//    pub address: NetAddress,
//}
//
///// Represents an outcome for the request to establish a new [PeerConnection].
/////
///// [PeerConnection]: ../../connection/peer_connection/index.html
//#[derive(Serialize, Deserialize, Debug)]
// pub enum ConnectRequestOutcome {
//    /// Accept response to a request to open a peer connection from a remote peer.
//    Accepted {
//        /// The zeroMQ Curve public key to use for the peer connection
//        curve_public_key: CurvePublicKey,
//        /// The address to which to connect
//        address: NetAddress,
//    },
//    /// Reject response to a request to open a peer connection from a remote peer.
//    Rejected(RejectReason),
//}
///// Represents the reason for a peer connection request being rejected
//#[derive(Error, Serialize, Deserialize, Debug, PartialEq, Eq)]
// pub enum RejectReason {
//    /// Peer already has an existing active peer connection
//    ExistingConnection,
//    /// A connection collision has been detected, foreign node should abandon the connection attempt
//    CollisionDetected,
//}

include_proto!("control_service");

impl MessageHeader {
    pub fn new(message_type: MessageType) -> Self {
        Self {
            message_type: message_type as i32,
        }
    }
}
