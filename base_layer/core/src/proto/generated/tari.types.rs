/// Define the data type that is used to store results of `HashDigest`
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HashOutput {
    #[prost(bytes, tag = "1")]
    pub data: std::vec::Vec<u8>,
}
/// Commitment wrapper
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Commitment {
    #[prost(bytes, tag = "1")]
    pub data: std::vec::Vec<u8>,
}
/// Define the explicit Signature implementation for the Tari base layer. A different signature scheme can be
/// employed by redefining this type.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Signature {
    #[prost(bytes, tag = "1")]
    pub public_nonce: std::vec::Vec<u8>,
    #[prost(bytes, tag = "2")]
    pub signature: std::vec::Vec<u8>,
}
/// BlindingFactor wrapper
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BlindingFactor {
    #[prost(bytes, tag = "1")]
    pub data: std::vec::Vec<u8>,
}
/// The transaction kernel tracks the excess for a given transaction. For an explanation of what the excess is, and
/// why it is necessary, refer to the
/// [Mimblewimble TLU post](https://tlu.tarilabs.com/protocols/mimblewimble-1/sources/PITCHME.link.html?highlight=mimblewimble#mimblewimble).
/// The kernel also tracks other transaction metadata, such as the lock height for the transaction (i.e. the earliest
/// this transaction can be mined) and the transaction fee, in cleartext.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TransactionKernel {
    /// Options for a kernel's structure or use
    #[prost(uint32, tag = "1")]
    pub features: u32,
    //// Fee originally included in the transaction this proof is for (in MicroTari)
    #[prost(uint64, tag = "2")]
    pub fee: u64,
    /// This kernel is not valid earlier than lock_height blocks
    /// The max lock_height of all *inputs* to this transaction
    #[prost(uint64, tag = "3")]
    pub lock_height: u64,
    /// This is an optional field used by committing to additional tx meta data between the two parties
    #[prost(message, optional, tag = "4")]
    pub meta_info: ::std::option::Option<HashOutput>,
    /// This is an optional field and is the hash of the kernel this kernel is linked to.
    /// This field is for example for relative time-locked transactions
    #[prost(message, optional, tag = "5")]
    pub linked_kernel: ::std::option::Option<HashOutput>,
    /// Remainder of the sum of all transaction commitments. If the transaction
    /// is well formed, amounts components should sum to zero and the excess
    /// is hence a valid public key.
    #[prost(message, optional, tag = "6")]
    pub excess: ::std::option::Option<Commitment>,
    /// The signature proving the excess is a valid public key, which signs
    /// the transaction fee.
    #[prost(message, optional, tag = "7")]
    pub excess_sig: ::std::option::Option<Signature>,
}
/// A transaction input.
///
/// Primarily a reference to an output being spent by the transaction.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TransactionInput {
    /// The features of the output being spent. We will check maturity for all outputs.
    #[prost(message, optional, tag = "1")]
    pub features: ::std::option::Option<OutputFeatures>,
    /// The commitment referencing the output being spent.
    #[prost(message, optional, tag = "2")]
    pub commitment: ::std::option::Option<Commitment>,
    //// The hash of the locking script on this UTXO.
    #[prost(message, optional, tag = "3")]
    pub script_hash: ::std::option::Option<HashOutput>,
}
/// Output for a transaction, defining the new ownership of coins that are being transferred. The commitment is a
/// blinded value for the output while the range proof guarantees the commitment includes a positive value without
/// overflow and the ownership of the private key.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TransactionOutput {
    /// Options for an output's structure or use
    #[prost(message, optional, tag = "1")]
    pub features: ::std::option::Option<OutputFeatures>,
    /// The homomorphic commitment representing the output amount
    #[prost(message, optional, tag = "2")]
    pub commitment: ::std::option::Option<Commitment>,
    /// A proof that the commitment is in the right range
    #[prost(bytes, tag = "3")]
    pub range_proof: std::vec::Vec<u8>,
    //// The hash of the locking script on this UTXO.
    #[prost(message, optional, tag = "4")]
    pub script_hash: ::std::option::Option<HashOutput>,
}
/// Options for UTXO's
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OutputFeatures {
    /// Flags are the feature flags that differentiate between outputs, eg Coinbase all of which has different rules
    #[prost(uint32, tag = "1")]
    pub flags: u32,
    /// The maturity of the specific UTXO. This is the min lock height at which an UTXO can be spend. Coinbase UTXO
    /// require a min maturity of the Coinbase_lock_height, this should be checked on receiving new blocks.
    #[prost(uint64, tag = "2")]
    pub maturity: u64,
}
/// The components of the block or transaction. The same struct can be used for either, since in Mimblewimble,
/// cut-through means that blocks and transactions have the same structure. The inputs, outputs and kernels should
/// be sorted by their Blake2b-256bit digest hash
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AggregateBody {
    /// List of inputs spent by the transaction.
    #[prost(message, repeated, tag = "1")]
    pub inputs: ::std::vec::Vec<TransactionInput>,
    /// List of outputs the transaction produces.
    #[prost(message, repeated, tag = "2")]
    pub outputs: ::std::vec::Vec<TransactionOutput>,
    /// Kernels contain the excesses and their signatures for transaction
    #[prost(message, repeated, tag = "3")]
    pub kernels: ::std::vec::Vec<TransactionKernel>,
}
/// A transaction which consists of a kernel offset and an aggregate body made up of inputs, outputs and kernels.
/// This struct is used to describe single transactions only.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Transaction {
    #[prost(message, optional, tag = "1")]
    pub offset: ::std::option::Option<BlindingFactor>,
    #[prost(message, optional, tag = "2")]
    pub body: ::std::option::Option<AggregateBody>,
}
