#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TransactionFinalizedMessage {
    /// The transaction id for the recipient
    #[prost(uint64, tag = "1")]
    pub tx_id: u64,
    /// The actual transaction;
    #[prost(message, optional, tag = "2")]
    pub transaction: ::std::option::Option<super::types::Transaction>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TransactionCancelledMessage {
    /// The transaction id for the cancelled transaction
    #[prost(uint64, tag = "1")]
    pub tx_id: u64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TransactionMetadata {
    /// The absolute fee for the transaction
    #[prost(uint64, tag = "1")]
    pub fee: u64,
    /// The earliest block this transaction can be mined
    #[prost(uint64, tag = "2")]
    pub lock_height: u64,
    /// This is an optional field used by committing to additional tx meta data between the two parties
    #[prost(message, optional, tag = "3")]
    pub meta_info: ::std::option::Option<super::types::HashOutput>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SingleRoundSenderData {
    /// The transaction id for the recipient
    #[prost(uint64, tag = "1")]
    pub tx_id: u64,
    /// The amount, in µT, being sent to the recipient
    #[prost(uint64, tag = "2")]
    pub amount: u64,
    /// The offset public excess for this transaction
    #[prost(bytes, tag = "3")]
    pub public_excess: std::vec::Vec<u8>,
    /// The sender's public nonce
    #[prost(bytes, tag = "4")]
    pub public_nonce: std::vec::Vec<u8>,
    /// The transaction metadata
    #[prost(message, optional, tag = "5")]
    pub metadata: ::std::option::Option<TransactionMetadata>,
    /// Plain text message to receiver
    #[prost(string, tag = "6")]
    pub message: std::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TransactionSenderMessage {
    #[prost(oneof = "transaction_sender_message::Message", tags = "1, 2, 3")]
    pub message: ::std::option::Option<transaction_sender_message::Message>,
}
pub mod transaction_sender_message {
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Message {
        #[prost(bool, tag = "1")]
        None(bool),
        #[prost(message, tag = "2")]
        Single(super::SingleRoundSenderData),
        // TODO: Three round types
        #[prost(bool, tag = "3")]
        Multiple(bool),
    }
}
/// This is the message containing the public data that the Receiver will send back to the Sender
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RecipientSignedMessage {
    #[prost(uint64, tag = "1")]
    pub tx_id: u64,
    #[prost(message, optional, tag = "2")]
    pub output: ::std::option::Option<super::types::TransactionOutput>,
    #[prost(bytes, tag = "3")]
    pub public_spend_key: std::vec::Vec<u8>,
    #[prost(message, optional, tag = "4")]
    pub partial_signature: ::std::option::Option<super::types::Signature>,
}
