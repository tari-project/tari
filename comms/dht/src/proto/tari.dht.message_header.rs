#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MessageHeader {
    /// Indicates a type of message. This can be any enum type
    #[prost(int32, tag = "1")]
    pub message_type: i32,
    #[prost(uint64, tag = "2")]
    pub nonce: u64,
}
