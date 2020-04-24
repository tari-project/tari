#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DhtHeader {
    #[prost(uint32, tag = "1")]
    pub version: u32,
    /// Origin public key of the message. This can be the same peer that sent the message
    /// or another peer if the message should be forwarded. This is optional but MUST be specified
    /// if the ENCRYPTED flag is set.
    /// If an ephemeral_public_key is specified, this MUST be encrypted using a derived ECDH shared key
    #[prost(bytes, tag = "5")]
    pub origin_mac: std::vec::Vec<u8>,
    /// Ephemeral public key component of the ECDH shared key. MUST be specified if the ENCRYPTED flag is set.
    #[prost(bytes, tag = "6")]
    pub ephemeral_public_key: std::vec::Vec<u8>,
    /// The type of message
    #[prost(enumeration = "DhtMessageType", tag = "7")]
    pub message_type: i32,
    /// The network for which this message is intended (e.g. TestNet, MainNet etc.)
    #[prost(enumeration = "Network", tag = "8")]
    pub network: i32,
    #[prost(uint32, tag = "9")]
    pub flags: u32,
    #[prost(oneof = "dht_header::Destination", tags = "2, 3, 4")]
    pub destination: ::std::option::Option<dht_header::Destination>,
}
pub mod dht_header {
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Destination {
        /// The sender has chosen not to disclose the message destination
        #[prost(bool, tag = "2")]
        Unknown(bool),
        /// Destined for a particular public key
        #[prost(bytes, tag = "3")]
        PublicKey(std::vec::Vec<u8>),
        /// Destined for a particular node id, or network region
        #[prost(bytes, tag = "4")]
        NodeId(std::vec::Vec<u8>),
    }
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DhtEnvelope {
    #[prost(message, optional, tag = "1")]
    pub header: ::std::option::Option<DhtHeader>,
    #[prost(bytes, tag = "2")]
    pub body: std::vec::Vec<u8>,
}
/// The Message Authentication Code (MAC) message format of the decrypted `DhtHeader::origin_mac` field
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OriginMac {
    #[prost(bytes, tag = "1")]
    pub public_key: std::vec::Vec<u8>,
    #[prost(bytes, tag = "2")]
    pub signature: std::vec::Vec<u8>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum DhtMessageType {
    /// Indicated this message is not a DHT message
    None = 0,
    /// Join Request
    Join = 1,
    /// Discovery request
    Discovery = 2,
    /// Response to a discovery request
    DiscoveryResponse = 3,
    /// Message was rejected
    RejectMsg = 4,
    /// Request stored messages from a node
    SafRequestMessages = 20,
    /// Stored messages response
    SafStoredMessages = 21,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum Network {
    /// Main net (default)
    MainNet = 0,
    /// Test net
    TestNet = 1,
    /// Network used for local tests
    LocalTest = 2,
}
