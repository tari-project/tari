#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MessageHeader {
    #[prost(enumeration = "MessageType", tag = "1")]
    pub message_type: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum MessageType {
    None = 0,
    RequestConnection = 1,
    Ping = 2,
    AcceptPeerConnection = 3,
    RejectPeerConnection = 4,
    Pong = 5,
    ConnectRequestOutcome = 6,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RequestConnectionMessage {
    #[prost(string, tag = "1")]
    pub control_service_address: std::string::String,
    #[prost(bytes, tag = "2")]
    pub node_id: std::vec::Vec<u8>,
    #[prost(uint64, tag = "3")]
    pub features: u64,
}
/// Represents an outcome for the request to establish a new [PeerConnection].
///
/// [PeerConnection]: ../../connection/peer_connection/index.html
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RequestConnectionOutcome {
    /// True if the connection is accepted, otherwise false
    #[prost(bool, tag = "1")]
    pub accepted: bool,
    /// The zeroMQ Curve public key to use for the peer connection
    #[prost(bytes, tag = "2")]
    pub curve_public_key: std::vec::Vec<u8>,
    //// The address of the open port to connect to
    #[prost(string, tag = "3")]
    pub address: std::string::String,
    //// If this connection was not accepted, the rejection reason is given
    #[prost(enumeration = "RejectReason", tag = "4")]
    pub reject_reason: i32,
}
/// Represents the reason for a peer connection request being rejected
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum RejectReason {
    /// No reject reason given
    None = 0,
    /// Peer already has an existing active peer connection
    ExistingConnection = 1,
    /// A connection collision has been detected, foreign node should abandon the connection attempt
    CollisionDetected = 2,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PingMessage {}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PongMessage {}
