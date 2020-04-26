/// JoinMessage contains the information required for a network join request.
///
/// Message containing contact information for a node wishing to join the network.
/// When this message is received the node validates the provided information and,
/// if everything checks out, the peer is added to the peer list and the join request
/// is propagated to the network.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct JoinMessage {
    #[prost(bytes, tag = "1")]
    pub node_id: std::vec::Vec<u8>,
    #[prost(string, repeated, tag = "2")]
    pub addresses: ::std::vec::Vec<std::string::String>,
    #[prost(uint64, tag = "3")]
    pub peer_features: u64,
    #[prost(uint64, tag = "4")]
    pub nonce: u64,
}
/// The DiscoverMessage stores the information required for a network discover request.
///
/// When this message is received and can be decrypted, this node verifies the information
/// provided and, if everything checks out, a DiscoveryResponse is sent to the origin of this
/// Discovery request so that the source node knows about this node.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DiscoveryMessage {
    #[prost(bytes, tag = "1")]
    pub node_id: std::vec::Vec<u8>,
    #[prost(string, repeated, tag = "2")]
    pub addresses: ::std::vec::Vec<std::string::String>,
    #[prost(uint64, tag = "3")]
    pub peer_features: u64,
    #[prost(uint64, tag = "4")]
    pub nonce: u64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DiscoveryResponseMessage {
    #[prost(bytes, tag = "1")]
    pub node_id: std::vec::Vec<u8>,
    #[prost(string, repeated, tag = "2")]
    pub addresses: ::std::vec::Vec<std::string::String>,
    #[prost(uint64, tag = "3")]
    pub peer_features: u64,
    #[prost(uint64, tag = "4")]
    pub nonce: u64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RejectMessage {
    /// The signature of the rejected message
    #[prost(bytes, tag = "1")]
    pub signature: std::vec::Vec<u8>,
    /// The reason for rejection
    #[prost(enumeration = "RejectMessageReason", tag = "2")]
    pub reason: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum RejectMessageReason {
    Unknown = 0,
    /// The destination node does not support the specified network
    UnsupportedNetwork = 1,
}
