/// A ping or pong message
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PingPongMessage {
    /// Indicates if this message is a ping or pong
    #[prost(enumeration = "PingPong", tag = "1")]
    pub ping_pong: i32,
    /// The nonce of the ping. Pong messages MUST use the nonce from a corresponding ping
    #[prost(uint64, tag = "2")]
    pub nonce: u64,
    /// Metadata attached to the message. The int32 key SHOULD always be one of the keys in `MetadataKey`.
    #[prost(map = "int32, bytes", tag = "3")]
    pub metadata: ::std::collections::HashMap<i32, std::vec::Vec<u8>>,
    /// Indicates the application version from which the message was sent
    #[prost(string, tag = "4")]
    pub useragent: std::string::String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum PingPong {
    Ping = 0,
    Pong = 1,
}
/// This enum represents all the possible metadata keys that can be used with a ping/pong message.
/// MetadataKey may be extended as the need arises.
///
/// _NOTE: Key values should NEVER be re-used_
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum MetadataKey {
    /// The default key. This should never be used as it represents the absence of a key.
    None = 0,
    /// The value for this key contains chain metadata
    ChainMetadata = 1,
}
