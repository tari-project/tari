/// Message type for all RPC requests
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RpcRequest {
    /// An identifier that is unique per request per session. This value is not strictly needed as
    /// requests and responses alternate on the protocol level without the need for storing and matching
    /// request IDs. However, this value is useful for logging.
    #[prost(uint32, tag = "1")]
    pub request_id: u32,
    /// The method identifier. The matching method for a given value is defined by each service.
    #[prost(uint32, tag = "2")]
    pub method: u32,
    /// Message flags. Currently this is not used for requests.
    #[prost(uint32, tag = "3")]
    pub flags: u32,
    /// The length of time in seconds that a client is willing to wait for a response
    #[prost(uint64, tag = "4")]
    pub deadline: u64,
    /// The message payload
    #[prost(bytes, tag = "10")]
    pub message: std::vec::Vec<u8>,
}
/// Message type for all RPC responses
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RpcResponse {
    /// The request ID of a prior request.
    #[prost(uint32, tag = "1")]
    pub request_id: u32,
    /// The status of the response. A non-zero status indicates an error.
    #[prost(uint32, tag = "2")]
    pub status: u32,
    /// Message flags. Currently only used to indicate if a stream of messages has completed.
    #[prost(uint32, tag = "3")]
    pub flags: u32,
    /// The message payload. If the status is non-zero, this contains additional error details.
    #[prost(bytes, tag = "10")]
    pub message: std::vec::Vec<u8>,
}
/// Message sent by the client when negotiating an RPC session. A server may close the substream if it does
/// not agree with the session parameters.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RpcSession {
    /// The RPC versions supported by the client
    #[prost(uint32, repeated, tag = "1")]
    pub supported_versions: ::std::vec::Vec<u32>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RpcSessionReply {
    #[prost(oneof = "rpc_session_reply::SessionResult", tags = "1, 2")]
    pub session_result: ::std::option::Option<rpc_session_reply::SessionResult>,
}
pub mod rpc_session_reply {
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum SessionResult {
        /// The RPC version selected by the server
        #[prost(uint32, tag = "1")]
        AcceptedVersion(u32),
        /// Indicates the server rejected the session
        #[prost(bool, tag = "2")]
        Rejected(bool),
    }
}
