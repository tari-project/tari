/// `get_closer_peers` request
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetCloserPeersRequest {
    /// The number of peers to return
    #[prost(uint32, tag = "1")]
    pub n: u32,
    #[prost(bytes, repeated, tag = "2")]
    pub excluded: ::std::vec::Vec<std::vec::Vec<u8>>,
    #[prost(bytes, tag = "3")]
    pub closer_to: std::vec::Vec<u8>,
    #[prost(bool, tag = "4")]
    pub include_clients: bool,
}
/// `get_peers` request
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetPeersRequest {
    /// The number of peers to return, 0 for all peers
    #[prost(uint32, tag = "1")]
    pub n: u32,
    #[prost(bool, tag = "2")]
    pub include_clients: bool,
}
/// GET peers response
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetPeersResponse {
    #[prost(message, optional, tag = "1")]
    pub peer: ::std::option::Option<Peer>,
}
/// Minimal peer information
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Peer {
    #[prost(bytes, tag = "1")]
    pub public_key: std::vec::Vec<u8>,
    #[prost(string, repeated, tag = "2")]
    pub addresses: ::std::vec::Vec<std::string::String>,
    #[prost(uint64, tag = "3")]
    pub peer_features: u64,
}
