#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PeerIdentityMsg {
    #[prost(bytes, tag = "1")]
    pub node_id: std::vec::Vec<u8>,
    #[prost(string, repeated, tag = "2")]
    pub addresses: ::std::vec::Vec<std::string::String>,
    #[prost(uint64, tag = "3")]
    pub features: u64,
    #[prost(bytes, repeated, tag = "4")]
    pub supported_protocols: ::std::vec::Vec<std::vec::Vec<u8>>,
}
