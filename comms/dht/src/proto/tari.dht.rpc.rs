/// GET peers request
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetPeersRequest {
    /// The number of peers to return
    #[prost(uint32, tag = "1")]
    pub n: u32,
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
