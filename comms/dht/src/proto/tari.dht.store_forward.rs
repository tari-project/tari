/// The RetrieveMessageRequest is used for requesting the set of stored messages from neighbouring peer nodes. If a
/// start_time is provided then only messages after the specified time will be sent, otherwise all applicable messages
/// will be sent.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct StoredMessagesRequest {
    #[prost(message, optional, tag = "1")]
    pub since: ::std::option::Option<::prost_types::Timestamp>,
}
/// Storage for a single message envelope, including the date and time when the element was stored
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct StoredMessage {
    #[prost(message, optional, tag = "1")]
    pub stored_at: ::std::option::Option<::prost_types::Timestamp>,
    #[prost(uint32, tag = "2")]
    pub version: u32,
    #[prost(message, optional, tag = "3")]
    pub dht_header: ::std::option::Option<super::envelope::DhtHeader>,
    #[prost(bytes, tag = "4")]
    pub encrypted_body: std::vec::Vec<u8>,
}
/// The StoredMessages contains the set of applicable messages retrieved from a neighbouring peer node.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct StoredMessagesResponse {
    #[prost(message, repeated, tag = "1")]
    pub messages: ::std::vec::Vec<StoredMessage>,
}
