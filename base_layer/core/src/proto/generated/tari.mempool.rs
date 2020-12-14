#[derive(Clone, PartialEq, ::prost::Message)]
pub struct StatsResponse {
    #[prost(uint64, tag = "1")]
    pub total_txs: u64,
    #[prost(uint64, tag = "2")]
    pub unconfirmed_txs: u64,
    #[prost(uint64, tag = "5")]
    pub reorg_txs: u64,
    #[prost(uint64, tag = "6")]
    pub total_weight: u64,
}
/// TODO: Remove duplicate Signature, transaction also has a Signature.
/// Define the explicit Signature implementation for the Tari base layer. A different signature scheme can be
/// employed by redefining this type.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Signature {
    #[prost(bytes, tag = "1")]
    pub public_nonce: std::vec::Vec<u8>,
    #[prost(bytes, tag = "2")]
    pub signature: std::vec::Vec<u8>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct StateResponse {
    /// List of transactions in unconfirmed pool.
    #[prost(message, repeated, tag = "1")]
    pub unconfirmed_pool: ::std::vec::Vec<Signature>,
    /// List of transactions in reorg pool.
    #[prost(message, repeated, tag = "4")]
    pub reorg_pool: ::std::vec::Vec<Signature>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TxStorage {
    #[prost(enumeration = "TxStorageResponse", tag = "1")]
    pub response: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum TxStorageResponse {
    None = 0,
    UnconfirmedPool = 1,
    ReorgPool = 4,
    NotStored = 5,
}
/// Response type for a received MempoolService requests
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MempoolServiceResponse {
    #[prost(uint64, tag = "1")]
    pub request_key: u64,
    #[prost(oneof = "mempool_service_response::Response", tags = "2, 3, 4")]
    pub response: ::std::option::Option<mempool_service_response::Response>,
}
pub mod mempool_service_response {
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Response {
        #[prost(message, tag = "2")]
        Stats(super::StatsResponse),
        #[prost(message, tag = "3")]
        State(super::StateResponse),
        #[prost(enumeration = "super::TxStorageResponse", tag = "4")]
        TxStorage(i32),
    }
}
/// Request type for a received MempoolService request.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MempoolServiceRequest {
    #[prost(uint64, tag = "1")]
    pub request_key: u64,
    #[prost(oneof = "mempool_service_request::Request", tags = "2, 3, 4, 5")]
    pub request: ::std::option::Option<mempool_service_request::Request>,
}
pub mod mempool_service_request {
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Request {
        /// Indicates a GetStats request. The value of the bool should be ignored.
        #[prost(bool, tag = "2")]
        GetStats(bool),
        /// Indicates a GetState request. The value of the bool should be ignored.
        #[prost(bool, tag = "3")]
        GetState(bool),
        /// Indicates a GetTxStateByExcessSig request.
        #[prost(message, tag = "4")]
        GetTxStateByExcessSig(super::super::types::Signature),
        /// Indicates a SubmitTransaction request.
        #[prost(message, tag = "5")]
        SubmitTransaction(super::super::types::Transaction),
    }
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TransactionInventory {
    /// A list of kernel excess sigs used to identify transactions
    #[prost(bytes, repeated, tag = "1")]
    pub items: ::std::vec::Vec<std::vec::Vec<u8>>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TransactionItem {
    #[prost(message, optional, tag = "1")]
    pub transaction: ::std::option::Option<super::types::Transaction>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct InventoryIndexes {
    #[prost(uint32, repeated, tag = "1")]
    pub indexes: ::std::vec::Vec<u32>,
}
