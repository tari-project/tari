/// The BlockHeader contains all the metadata for the block, including proof of work, a link to the previous block
/// and the transaction kernels.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BlockHeader {
    /// The hash of the block
    #[prost(bytes, tag = "1")]
    pub hash: std::vec::Vec<u8>,
    /// Version of the block
    #[prost(uint32, tag = "2")]
    pub version: u32,
    /// Height of this block since the genesis block (height 0)
    #[prost(uint64, tag = "3")]
    pub height: u64,
    /// Hash of the block previous to this in the chain.
    #[prost(bytes, tag = "4")]
    pub prev_hash: std::vec::Vec<u8>,
    /// Timestamp at which the block was built.
    #[prost(message, optional, tag = "5")]
    pub timestamp: ::std::option::Option<::prost_types::Timestamp>,
    /// This is the UTXO merkle root of the outputs
    /// This is calculated as Hash (txo MMR root  || roaring bitmap hash of UTXO indices)
    #[prost(bytes, tag = "6")]
    pub output_mr: std::vec::Vec<u8>,
    /// This is the MMR root of the range proofs
    #[prost(bytes, tag = "7")]
    pub range_proof_mr: std::vec::Vec<u8>,
    /// This is the MMR root of the kernels
    #[prost(bytes, tag = "8")]
    pub kernel_mr: std::vec::Vec<u8>,
    /// Total accumulated sum of kernel offsets since genesis block. We can derive the kernel offset sum for *this*
    /// block from the total kernel offset of the previous block header.
    #[prost(bytes, tag = "9")]
    pub total_kernel_offset: std::vec::Vec<u8>,
    /// Nonce increment used to mine this block.
    #[prost(uint64, tag = "10")]
    pub nonce: u64,
    /// Proof of work metadata
    #[prost(message, optional, tag = "11")]
    pub pow: ::std::option::Option<ProofOfWork>,
}
/// Metadata required for validating the Proof of Work calculation
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ProofOfWork {
    /// 0 = Monero
    /// 1 = Blake
    #[prost(uint64, tag = "1")]
    pub pow_algo: u64,
    #[prost(uint64, tag = "2")]
    pub accumulated_monero_difficulty: u64,
    #[prost(uint64, tag = "3")]
    pub accumulated_blake_difficulty: u64,
    #[prost(bytes, tag = "4")]
    pub pow_data: std::vec::Vec<u8>,
    #[prost(uint64, tag = "5")]
    pub target_difficulty: u64,
}
/// This is used to request the which pow algo should be used with the block template
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PowAlgo {
    #[prost(enumeration = "pow_algo::PowAlgo", tag = "1")]
    pub pow_algo: i32,
}
pub mod pow_algo {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
    #[repr(i32)]
    pub enum PowAlgo {
        Monero = 0,
        Blake = 1,
    }
}
/// A Tari block. Blocks are linked together into a blockchain.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Block {
    #[prost(message, optional, tag = "1")]
    pub header: ::std::option::Option<BlockHeader>,
    #[prost(message, optional, tag = "2")]
    pub body: ::std::option::Option<AggregateBody>,
}
/// The representation of a historical block in the blockchain. It is essentially identical to a protocol-defined
/// block but contains some extra metadata that clients such as Block Explorers will find interesting.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HistoricalBlock {
    /// The number of blocks that have been mined since this block, including this one. The current tip will have one
    /// confirmation.
    #[prost(uint64, tag = "1")]
    pub confirmations: u64,
    /// An array of commitments of the outputs from this block that have subsequently been spent.
    #[prost(bytes, repeated, tag = "2")]
    pub spent_commitments: ::std::vec::Vec<std::vec::Vec<u8>>,
    /// The underlying block
    #[prost(message, optional, tag = "3")]
    pub block: ::std::option::Option<Block>,
}
/// The NewBlockHeaderTemplate is used for the construction of a new mineable block. It contains all the metadata for
/// the block that the Base Node is able to complete on behalf of a Miner.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewBlockHeaderTemplate {
    /// Version of the block
    #[prost(uint32, tag = "1")]
    pub version: u32,
    /// Height of this block since the genesis block (height 0)
    #[prost(uint64, tag = "2")]
    pub height: u64,
    /// Hash of the block previous to this in the chain.
    #[prost(bytes, tag = "3")]
    pub prev_hash: std::vec::Vec<u8>,
    /// Total accumulated sum of kernel offsets since genesis block. We can derive the kernel offset sum for *this*
    /// block from the total kernel offset of the previous block header.
    #[prost(bytes, tag = "4")]
    pub total_kernel_offset: std::vec::Vec<u8>,
    /// Proof of work metadata
    #[prost(message, optional, tag = "5")]
    pub pow: ::std::option::Option<ProofOfWork>,
}
/// The new block template is used constructing a new partial block, allowing a miner to added the coinbase utxo and as
/// a final step the Base node to add the MMR roots to the header.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewBlockTemplate {
    #[prost(message, optional, tag = "1")]
    pub header: ::std::option::Option<NewBlockHeaderTemplate>,
    #[prost(message, optional, tag = "2")]
    pub body: ::std::option::Option<AggregateBody>,
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
    #[prost(bytes, tag = "4")]
    pub meta_info: std::vec::Vec<u8>,
    /// This is an optional field and is the hash of the kernel this kernel is linked to.
    /// This field is for example for relative time-locked transactions
    #[prost(bytes, tag = "5")]
    pub linked_kernel: std::vec::Vec<u8>,
    /// Remainder of the sum of all transaction commitments. If the transaction
    /// is well formed, amounts components should sum to zero and the excess
    /// is hence a valid public key.
    #[prost(bytes, tag = "6")]
    pub excess: std::vec::Vec<u8>,
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
    #[prost(bytes, tag = "2")]
    pub commitment: std::vec::Vec<u8>,
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
    #[prost(bytes, tag = "2")]
    pub commitment: std::vec::Vec<u8>,
    /// A proof that the commitment is in the right range
    #[prost(bytes, tag = "3")]
    pub range_proof: std::vec::Vec<u8>,
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
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Transaction {
    #[prost(bytes, tag = "1")]
    pub offset: std::vec::Vec<u8>,
    #[prost(message, optional, tag = "2")]
    pub body: ::std::option::Option<AggregateBody>,
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
//// Consensus Constants response
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ConsensusConstants {
    //// The min height maturity a coinbase utxo must have
    #[prost(uint64, tag = "1")]
    pub coinbase_lock_height: u64,
    //// Current version of the blockchain
    #[prost(uint32, tag = "2")]
    pub blockchain_version: u32,
    //// The Future Time Limit (FTL) of the blockchain in seconds. This is the max allowable timestamp that is
    //// excepted. We use TxN/20 where T = target time = 60 seconds, and N = block_window = 150
    #[prost(uint64, tag = "3")]
    pub future_time_limit: u64,
    //// This is the our target time in seconds between blocks
    #[prost(uint64, tag = "4")]
    pub target_block_interval: u64,
    //// When doing difficulty adjustments and FTL calculations this is the amount of blocks we look at
    #[prost(uint64, tag = "5")]
    pub difficulty_block_window: u64,
    //// When doing difficulty adjustments, this is the maximum block time allowed
    #[prost(uint64, tag = "6")]
    pub difficulty_max_block_interval: u64,
    //// Maximum transaction weight used for the construction of new blocks.
    #[prost(uint64, tag = "7")]
    pub max_block_transaction_weight: u64,
    //// The amount of PoW algorithms used by the Tari chain.
    #[prost(uint64, tag = "8")]
    pub pow_algo_count: u64,
    //// This is how many blocks we use to count towards the median timestamp to ensure the block chain moves forward
    #[prost(uint64, tag = "9")]
    pub median_timestamp_count: u64,
    //// This is the initial emission curve amount
    #[prost(uint64, tag = "10")]
    pub emission_initial: u64,
    //// This is the emission curve delay
    #[prost(double, tag = "11")]
    pub emission_decay: f64,
    //// This is the emission curve tail amount
    #[prost(uint64, tag = "12")]
    pub emission_tail: u64,
    //// This is the initial min difficulty for the difficulty adjustment
    #[prost(uint64, tag = "13")]
    pub min_blake_pow_difficulty: u64,
    //// Block weight for inputs
    #[prost(uint64, tag = "14")]
    pub block_weight_inputs: u64,
    //// Block weight for output
    #[prost(uint64, tag = "15")]
    pub block_weight_outputs: u64,
    //// Block weight for kernels
    #[prost(uint64, tag = "16")]
    pub block_weight_kernels: u64,
}
//// return type of GetTipInfo
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TipInfoResponse {
    #[prost(message, optional, tag = "1")]
    pub metadata: ::std::option::Option<MetaData>,
}
//// return type of GetNewBlockTemplate
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewBlockTemplateResponse {
    #[prost(message, optional, tag = "1")]
    pub new_block_template: ::std::option::Option<NewBlockTemplate>,
    #[prost(uint64, tag = "2")]
    pub block_reward: u64,
}
//// An Empty placeholder for endpoints without request parameters
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Empty {}
/// Network difficulty response
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NetworkDifficultyResponse {
    #[prost(uint64, tag = "1")]
    pub difficulty: u64,
    #[prost(uint64, tag = "2")]
    pub estimated_hash_rate: u64,
    #[prost(uint64, tag = "3")]
    pub height: u64,
    #[prost(uint64, tag = "4")]
    pub timestamp: u64,
}
/// A generic single value response for a specific height
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ValueAtHeightResponse {
    #[prost(uint64, tag = "1")]
    pub value: u64,
    #[prost(uint64, tag = "2")]
    pub height: u64,
}
/// A generic uint value
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct IntegerValue {
    #[prost(uint64, tag = "1")]
    pub value: u64,
}
/// A generic String value
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct StringValue {
    #[prost(string, tag = "1")]
    pub value: std::string::String,
}
//// GetBlockSize / GetBlockFees Request
//// Either the starting and ending heights OR the from_tip param must be specified
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BlockGroupRequest {
    /// The height from the chain tip (optional)
    #[prost(uint64, tag = "1")]
    pub from_tip: u64,
    /// The starting height (optional)
    #[prost(uint64, tag = "2")]
    pub start_height: u64,
    /// The ending height (optional)
    #[prost(uint64, tag = "3")]
    pub end_height: u64,
    //// The type of calculation required (optional)
    //// Defaults to median
    //// median, mean, quartile, quantile
    #[prost(enumeration = "CalcType", tag = "4")]
    pub calc_type: i32,
}
//// GetBlockSize / GetBlockFees  Response
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BlockGroupResponse {
    #[prost(double, repeated, tag = "1")]
    pub value: ::std::vec::Vec<f64>,
    #[prost(enumeration = "CalcType", tag = "2")]
    pub calc_type: i32,
}
/// The request used for querying a function that requires a height, either between 2 points or from the chain tip
/// If start_height and end_height are set and > 0, they take precedence, otherwise from_tip is used
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HeightRequest {
    /// The height from the chain tip (optional)
    #[prost(uint64, tag = "1")]
    pub from_tip: u64,
    /// The starting height (optional)
    #[prost(uint64, tag = "2")]
    pub start_height: u64,
    /// The ending height (optional)
    #[prost(uint64, tag = "3")]
    pub end_height: u64,
}
/// The return type of the rpc GetCalcTiming
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CalcTimingResponse {
    #[prost(uint64, tag = "1")]
    pub max: u64,
    #[prost(uint64, tag = "2")]
    pub min: u64,
    #[prost(double, tag = "3")]
    pub avg: f64,
}
/// The request used for querying headers from the base node. The parameters `from_height` and `num_headers` can be used
/// to page through the current best chain.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ListHeadersRequest {
    /// The height to start at. Depending on sorting, will either default to use the tip or genesis block, for
    /// `SORTING_DESC` and `SORTING_ASC` respectively, if a value is not provided. The first header returned will
    /// be at this height followed by `num_headers` - 1 headers in the direction specified by `sorting`. If greater
    /// than the current tip, the current tip will be used.
    #[prost(uint64, tag = "1")]
    pub from_height: u64,
    /// The number of headers to return. If not specified, it will default to 10
    #[prost(uint64, tag = "2")]
    pub num_headers: u64,
    /// The ordering to return the headers in. If not specified will default to SORTING_DESC. Note that if
    /// `from_height` is not specified or is 0, if `sorting` is SORTING_DESC, the tip will be used as
    /// `from_height`, otherwise the block at height 0 will be used.
    #[prost(enumeration = "Sorting", tag = "3")]
    pub sorting: i32,
}
/// The request used for querying blocks in the base node's current best chain. Currently only querying by height is
/// available. Multiple blocks may be queried.e.g. [189092,100023,122424]. The order in which they are returned is not
/// guarenteed.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetBlocksRequest {
    #[prost(uint64, repeated, tag = "1")]
    pub heights: ::std::vec::Vec<u64>,
}
/// The return type of the rpc GetBlocks. Blocks are not guaranteed to be returned in the order requested.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetBlocksResponse {
    #[prost(message, repeated, tag = "1")]
    pub blocks: ::std::vec::Vec<HistoricalBlock>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MetaData {
    /// The current chain height, or the block number of the longest valid chain, or `None` if there is no chain
    #[prost(uint64, tag = "1")]
    pub height_of_longest_chain: u64,
    /// The block hash of the current tip of the longest valid chain, or `None` for an empty chain
    #[prost(bytes, tag = "2")]
    pub best_block: std::vec::Vec<u8>,
    /// The number of blocks back from the tip that this database tracks. A value of 0 indicates that all blocks are
    /// tracked (i.e. the database is in full archival mode).
    #[prost(uint64, tag = "4")]
    pub pruning_horizon: u64,
    /// The current geometric mean of the pow of the chain tip, or `None` if there is no chain
    #[prost(uint64, tag = "5")]
    pub accumulated_difficulty: u64,
}
/// This is the message that is returned for a miner after it asks for a new block.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetNewBlockResult {
    /// This is the header hash of the complated block
    #[prost(bytes, tag = "1")]
    pub block_hash: std::vec::Vec<u8>,
    /// This is the completed block
    #[prost(message, optional, tag = "2")]
    pub block: ::std::option::Option<Block>,
    /// This is the merge_mining hash of the completed block.
    #[prost(message, optional, tag = "3")]
    pub mining_data: ::std::option::Option<MinerData>,
}
/// This is mining data for the miner asking for a new block
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MinerData {
    #[prost(message, optional, tag = "1")]
    pub algo: ::std::option::Option<PowAlgo>,
    #[prost(uint64, tag = "2")]
    pub target_difficulty: u64,
    #[prost(uint64, tag = "3")]
    pub reward: u64,
    #[prost(bytes, tag = "4")]
    pub mergemining_hash: std::vec::Vec<u8>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum CalcType {
    Mean = 0,
    Median = 1,
    Quantile = 2,
    Quartile = 3,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum Sorting {
    Desc = 0,
    Asc = 1,
}
#[doc = r" Generated client implementations."]
pub mod base_node_client {
    #![allow(unused_variables, dead_code, missing_docs)]
    use tonic::codegen::*;
    #[doc = " The gRPC interface for interacting with the base node."]
    pub struct BaseNodeClient<T> {
        inner: tonic::client::Grpc<T>,
    }
    impl BaseNodeClient<tonic::transport::Channel> {
        #[doc = r" Attempt to create a new client by connecting to a given endpoint."]
        pub async fn connect<D>(dst: D) -> Result<Self, tonic::transport::Error>
        where
            D: std::convert::TryInto<tonic::transport::Endpoint>,
            D::Error: Into<StdError>,
        {
            let conn = tonic::transport::Endpoint::new(dst)?.connect().await?;
            Ok(Self::new(conn))
        }
    }
    impl<T> BaseNodeClient<T>
    where
        T: tonic::client::GrpcService<tonic::body::BoxBody>,
        T::ResponseBody: Body + HttpBody + Send + 'static,
        T::Error: Into<StdError>,
        <T::ResponseBody as HttpBody>::Error: Into<StdError> + Send,
    {
        pub fn new(inner: T) -> Self {
            let inner = tonic::client::Grpc::new(inner);
            Self { inner }
        }

        pub fn with_interceptor(inner: T, interceptor: impl Into<tonic::Interceptor>) -> Self {
            let inner = tonic::client::Grpc::with_interceptor(inner, interceptor);
            Self { inner }
        }

        #[doc = " Lists headers in the current best chain"]
        pub async fn list_headers(
            &mut self,
            request: impl tonic::IntoRequest<super::ListHeadersRequest>,
        ) -> Result<tonic::Response<tonic::codec::Streaming<super::BlockHeader>>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(tonic::Code::Unknown, format!("Service was not ready: {}", e.into()))
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/tari.rpc.BaseNode/ListHeaders");
            self.inner.server_streaming(request.into_request(), path, codec).await
        }

        #[doc = " Returns blocks in the current best chain. Currently only supports querying by height"]
        pub async fn get_blocks(
            &mut self,
            request: impl tonic::IntoRequest<super::GetBlocksRequest>,
        ) -> Result<tonic::Response<tonic::codec::Streaming<super::HistoricalBlock>>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(tonic::Code::Unknown, format!("Service was not ready: {}", e.into()))
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/tari.rpc.BaseNode/GetBlocks");
            self.inner.server_streaming(request.into_request(), path, codec).await
        }

        #[doc = " Returns the calc timing for the chain heights"]
        pub async fn get_calc_timing(
            &mut self,
            request: impl tonic::IntoRequest<super::HeightRequest>,
        ) -> Result<tonic::Response<super::CalcTimingResponse>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(tonic::Code::Unknown, format!("Service was not ready: {}", e.into()))
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/tari.rpc.BaseNode/GetCalcTiming");
            self.inner.unary(request.into_request(), path, codec).await
        }

        #[doc = " Returns the network Constants"]
        pub async fn get_constants(
            &mut self,
            request: impl tonic::IntoRequest<super::Empty>,
        ) -> Result<tonic::Response<super::ConsensusConstants>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(tonic::Code::Unknown, format!("Service was not ready: {}", e.into()))
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/tari.rpc.BaseNode/GetConstants");
            self.inner.unary(request.into_request(), path, codec).await
        }

        #[doc = " Returns Block Sizes"]
        pub async fn get_block_size(
            &mut self,
            request: impl tonic::IntoRequest<super::BlockGroupRequest>,
        ) -> Result<tonic::Response<super::BlockGroupResponse>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(tonic::Code::Unknown, format!("Service was not ready: {}", e.into()))
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/tari.rpc.BaseNode/GetBlockSize");
            self.inner.unary(request.into_request(), path, codec).await
        }

        #[doc = " Returns Block Fees"]
        pub async fn get_block_fees(
            &mut self,
            request: impl tonic::IntoRequest<super::BlockGroupRequest>,
        ) -> Result<tonic::Response<super::BlockGroupResponse>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(tonic::Code::Unknown, format!("Service was not ready: {}", e.into()))
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/tari.rpc.BaseNode/GetBlockFees");
            self.inner.unary(request.into_request(), path, codec).await
        }

        #[doc = " Get Version"]
        pub async fn get_version(
            &mut self,
            request: impl tonic::IntoRequest<super::Empty>,
        ) -> Result<tonic::Response<super::StringValue>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(tonic::Code::Unknown, format!("Service was not ready: {}", e.into()))
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/tari.rpc.BaseNode/GetVersion");
            self.inner.unary(request.into_request(), path, codec).await
        }

        #[doc = " Get coins in circulation"]
        pub async fn get_tokens_in_circulation(
            &mut self,
            request: impl tonic::IntoRequest<super::GetBlocksRequest>,
        ) -> Result<tonic::Response<tonic::codec::Streaming<super::ValueAtHeightResponse>>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(tonic::Code::Unknown, format!("Service was not ready: {}", e.into()))
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/tari.rpc.BaseNode/GetTokensInCirculation");
            self.inner.server_streaming(request.into_request(), path, codec).await
        }

        #[doc = " Get network difficulties"]
        pub async fn get_network_difficulty(
            &mut self,
            request: impl tonic::IntoRequest<super::HeightRequest>,
        ) -> Result<tonic::Response<tonic::codec::Streaming<super::NetworkDifficultyResponse>>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(tonic::Code::Unknown, format!("Service was not ready: {}", e.into()))
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/tari.rpc.BaseNode/GetNetworkDifficulty");
            self.inner.server_streaming(request.into_request(), path, codec).await
        }

        #[doc = " Get the block template"]
        pub async fn get_new_block_template(
            &mut self,
            request: impl tonic::IntoRequest<super::PowAlgo>,
        ) -> Result<tonic::Response<super::NewBlockTemplateResponse>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(tonic::Code::Unknown, format!("Service was not ready: {}", e.into()))
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/tari.rpc.BaseNode/GetNewBlockTemplate");
            self.inner.unary(request.into_request(), path, codec).await
        }

        #[doc = " Construct a new block from a provided template"]
        pub async fn get_new_block(
            &mut self,
            request: impl tonic::IntoRequest<super::NewBlockTemplate>,
        ) -> Result<tonic::Response<super::GetNewBlockResult>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(tonic::Code::Unknown, format!("Service was not ready: {}", e.into()))
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/tari.rpc.BaseNode/GetNewBlock");
            self.inner.unary(request.into_request(), path, codec).await
        }

        #[doc = " Submit a new block for propogation"]
        pub async fn submit_block(
            &mut self,
            request: impl tonic::IntoRequest<super::Block>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(tonic::Code::Unknown, format!("Service was not ready: {}", e.into()))
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/tari.rpc.BaseNode/SubmitBlock");
            self.inner.unary(request.into_request(), path, codec).await
        }

        #[doc = " Get the base node tip information"]
        pub async fn get_tip_info(
            &mut self,
            request: impl tonic::IntoRequest<super::Empty>,
        ) -> Result<tonic::Response<super::TipInfoResponse>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(tonic::Code::Unknown, format!("Service was not ready: {}", e.into()))
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/tari.rpc.BaseNode/GetTipInfo");
            self.inner.unary(request.into_request(), path, codec).await
        }
    }
    impl<T: Clone> Clone for BaseNodeClient<T> {
        fn clone(&self) -> Self {
            Self {
                inner: self.inner.clone(),
            }
        }
    }
    impl<T> std::fmt::Debug for BaseNodeClient<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "BaseNodeClient {{ ... }}")
        }
    }
}
#[doc = r" Generated server implementations."]
pub mod base_node_server {
    #![allow(unused_variables, dead_code, missing_docs)]
    use tonic::codegen::*;
    #[doc = "Generated trait containing gRPC methods that should be implemented for use with BaseNodeServer."]
    #[async_trait]
    pub trait BaseNode: Send + Sync + 'static {
        #[doc = "Server streaming response type for the ListHeaders method."]
        type ListHeadersStream: Stream<Item = Result<super::BlockHeader, tonic::Status>> + Send + Sync + 'static;
        #[doc = " Lists headers in the current best chain"]
        async fn list_headers(
            &self,
            request: tonic::Request<super::ListHeadersRequest>,
        ) -> Result<tonic::Response<Self::ListHeadersStream>, tonic::Status>;
        #[doc = "Server streaming response type for the GetBlocks method."]
        type GetBlocksStream: Stream<Item = Result<super::HistoricalBlock, tonic::Status>> + Send + Sync + 'static;
        #[doc = " Returns blocks in the current best chain. Currently only supports querying by height"]
        async fn get_blocks(
            &self,
            request: tonic::Request<super::GetBlocksRequest>,
        ) -> Result<tonic::Response<Self::GetBlocksStream>, tonic::Status>;
        #[doc = " Returns the calc timing for the chain heights"]
        async fn get_calc_timing(
            &self,
            request: tonic::Request<super::HeightRequest>,
        ) -> Result<tonic::Response<super::CalcTimingResponse>, tonic::Status>;
        #[doc = " Returns the network Constants"]
        async fn get_constants(
            &self,
            request: tonic::Request<super::Empty>,
        ) -> Result<tonic::Response<super::ConsensusConstants>, tonic::Status>;
        #[doc = " Returns Block Sizes"]
        async fn get_block_size(
            &self,
            request: tonic::Request<super::BlockGroupRequest>,
        ) -> Result<tonic::Response<super::BlockGroupResponse>, tonic::Status>;
        #[doc = " Returns Block Fees"]
        async fn get_block_fees(
            &self,
            request: tonic::Request<super::BlockGroupRequest>,
        ) -> Result<tonic::Response<super::BlockGroupResponse>, tonic::Status>;
        #[doc = " Get Version"]
        async fn get_version(
            &self,
            request: tonic::Request<super::Empty>,
        ) -> Result<tonic::Response<super::StringValue>, tonic::Status>;
        #[doc = "Server streaming response type for the GetTokensInCirculation method."]
        type GetTokensInCirculationStream: Stream<Item = Result<super::ValueAtHeightResponse, tonic::Status>>
            + Send
            + Sync
            + 'static;
        #[doc = " Get coins in circulation"]
        async fn get_tokens_in_circulation(
            &self,
            request: tonic::Request<super::GetBlocksRequest>,
        ) -> Result<tonic::Response<Self::GetTokensInCirculationStream>, tonic::Status>;
        #[doc = "Server streaming response type for the GetNetworkDifficulty method."]
        type GetNetworkDifficultyStream: Stream<Item = Result<super::NetworkDifficultyResponse, tonic::Status>>
            + Send
            + Sync
            + 'static;
        #[doc = " Get network difficulties"]
        async fn get_network_difficulty(
            &self,
            request: tonic::Request<super::HeightRequest>,
        ) -> Result<tonic::Response<Self::GetNetworkDifficultyStream>, tonic::Status>;
        #[doc = " Get the block template"]
        async fn get_new_block_template(
            &self,
            request: tonic::Request<super::PowAlgo>,
        ) -> Result<tonic::Response<super::NewBlockTemplateResponse>, tonic::Status>;
        #[doc = " Construct a new block from a provided template"]
        async fn get_new_block(
            &self,
            request: tonic::Request<super::NewBlockTemplate>,
        ) -> Result<tonic::Response<super::GetNewBlockResult>, tonic::Status>;
        #[doc = " Submit a new block for propogation"]
        async fn submit_block(
            &self,
            request: tonic::Request<super::Block>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status>;
        #[doc = " Get the base node tip information"]
        async fn get_tip_info(
            &self,
            request: tonic::Request<super::Empty>,
        ) -> Result<tonic::Response<super::TipInfoResponse>, tonic::Status>;
    }
    #[doc = " The gRPC interface for interacting with the base node."]
    #[derive(Debug)]
    #[doc(hidden)]
    pub struct BaseNodeServer<T: BaseNode> {
        inner: _Inner<T>,
    }
    struct _Inner<T>(Arc<T>, Option<tonic::Interceptor>);
    impl<T: BaseNode> BaseNodeServer<T> {
        pub fn new(inner: T) -> Self {
            let inner = Arc::new(inner);
            let inner = _Inner(inner, None);
            Self { inner }
        }

        pub fn with_interceptor(inner: T, interceptor: impl Into<tonic::Interceptor>) -> Self {
            let inner = Arc::new(inner);
            let inner = _Inner(inner, Some(interceptor.into()));
            Self { inner }
        }
    }
    impl<T, B> Service<http::Request<B>> for BaseNodeServer<T>
    where
        T: BaseNode,
        B: HttpBody + Send + Sync + 'static,
        B::Error: Into<StdError> + Send + 'static,
    {
        type Error = Never;
        type Future = BoxFuture<Self::Response, Self::Error>;
        type Response = http::Response<tonic::body::BoxBody>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: http::Request<B>) -> Self::Future {
            let inner = self.inner.clone();
            match req.uri().path() {
                "/tari.rpc.BaseNode/ListHeaders" => {
                    #[allow(non_camel_case_types)]
                    struct ListHeadersSvc<T: BaseNode>(pub Arc<T>);
                    impl<T: BaseNode> tonic::server::ServerStreamingService<super::ListHeadersRequest> for ListHeadersSvc<T> {
                        type Future = BoxFuture<tonic::Response<Self::ResponseStream>, tonic::Status>;
                        type Response = super::BlockHeader;
                        type ResponseStream = T::ListHeadersStream;

                        fn call(&mut self, request: tonic::Request<super::ListHeadersRequest>) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { inner.list_headers(request).await };
                            Box::pin(fut)
                        }
                    }
                    let inner = self.inner.clone();
                    let fut = async move {
                        let interceptor = inner.1;
                        let inner = inner.0;
                        let method = ListHeadersSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = if let Some(interceptor) = interceptor {
                            tonic::server::Grpc::with_interceptor(codec, interceptor)
                        } else {
                            tonic::server::Grpc::new(codec)
                        };
                        let res = grpc.server_streaming(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                },
                "/tari.rpc.BaseNode/GetBlocks" => {
                    #[allow(non_camel_case_types)]
                    struct GetBlocksSvc<T: BaseNode>(pub Arc<T>);
                    impl<T: BaseNode> tonic::server::ServerStreamingService<super::GetBlocksRequest> for GetBlocksSvc<T> {
                        type Future = BoxFuture<tonic::Response<Self::ResponseStream>, tonic::Status>;
                        type Response = super::HistoricalBlock;
                        type ResponseStream = T::GetBlocksStream;

                        fn call(&mut self, request: tonic::Request<super::GetBlocksRequest>) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { inner.get_blocks(request).await };
                            Box::pin(fut)
                        }
                    }
                    let inner = self.inner.clone();
                    let fut = async move {
                        let interceptor = inner.1;
                        let inner = inner.0;
                        let method = GetBlocksSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = if let Some(interceptor) = interceptor {
                            tonic::server::Grpc::with_interceptor(codec, interceptor)
                        } else {
                            tonic::server::Grpc::new(codec)
                        };
                        let res = grpc.server_streaming(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                },
                "/tari.rpc.BaseNode/GetCalcTiming" => {
                    #[allow(non_camel_case_types)]
                    struct GetCalcTimingSvc<T: BaseNode>(pub Arc<T>);
                    impl<T: BaseNode> tonic::server::UnaryService<super::HeightRequest> for GetCalcTimingSvc<T> {
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        type Response = super::CalcTimingResponse;

                        fn call(&mut self, request: tonic::Request<super::HeightRequest>) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { inner.get_calc_timing(request).await };
                            Box::pin(fut)
                        }
                    }
                    let inner = self.inner.clone();
                    let fut = async move {
                        let interceptor = inner.1.clone();
                        let inner = inner.0;
                        let method = GetCalcTimingSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = if let Some(interceptor) = interceptor {
                            tonic::server::Grpc::with_interceptor(codec, interceptor)
                        } else {
                            tonic::server::Grpc::new(codec)
                        };
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                },
                "/tari.rpc.BaseNode/GetConstants" => {
                    #[allow(non_camel_case_types)]
                    struct GetConstantsSvc<T: BaseNode>(pub Arc<T>);
                    impl<T: BaseNode> tonic::server::UnaryService<super::Empty> for GetConstantsSvc<T> {
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        type Response = super::ConsensusConstants;

                        fn call(&mut self, request: tonic::Request<super::Empty>) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { inner.get_constants(request).await };
                            Box::pin(fut)
                        }
                    }
                    let inner = self.inner.clone();
                    let fut = async move {
                        let interceptor = inner.1.clone();
                        let inner = inner.0;
                        let method = GetConstantsSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = if let Some(interceptor) = interceptor {
                            tonic::server::Grpc::with_interceptor(codec, interceptor)
                        } else {
                            tonic::server::Grpc::new(codec)
                        };
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                },
                "/tari.rpc.BaseNode/GetBlockSize" => {
                    #[allow(non_camel_case_types)]
                    struct GetBlockSizeSvc<T: BaseNode>(pub Arc<T>);
                    impl<T: BaseNode> tonic::server::UnaryService<super::BlockGroupRequest> for GetBlockSizeSvc<T> {
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        type Response = super::BlockGroupResponse;

                        fn call(&mut self, request: tonic::Request<super::BlockGroupRequest>) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { inner.get_block_size(request).await };
                            Box::pin(fut)
                        }
                    }
                    let inner = self.inner.clone();
                    let fut = async move {
                        let interceptor = inner.1.clone();
                        let inner = inner.0;
                        let method = GetBlockSizeSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = if let Some(interceptor) = interceptor {
                            tonic::server::Grpc::with_interceptor(codec, interceptor)
                        } else {
                            tonic::server::Grpc::new(codec)
                        };
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                },
                "/tari.rpc.BaseNode/GetBlockFees" => {
                    #[allow(non_camel_case_types)]
                    struct GetBlockFeesSvc<T: BaseNode>(pub Arc<T>);
                    impl<T: BaseNode> tonic::server::UnaryService<super::BlockGroupRequest> for GetBlockFeesSvc<T> {
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        type Response = super::BlockGroupResponse;

                        fn call(&mut self, request: tonic::Request<super::BlockGroupRequest>) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { inner.get_block_fees(request).await };
                            Box::pin(fut)
                        }
                    }
                    let inner = self.inner.clone();
                    let fut = async move {
                        let interceptor = inner.1.clone();
                        let inner = inner.0;
                        let method = GetBlockFeesSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = if let Some(interceptor) = interceptor {
                            tonic::server::Grpc::with_interceptor(codec, interceptor)
                        } else {
                            tonic::server::Grpc::new(codec)
                        };
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                },
                "/tari.rpc.BaseNode/GetVersion" => {
                    #[allow(non_camel_case_types)]
                    struct GetVersionSvc<T: BaseNode>(pub Arc<T>);
                    impl<T: BaseNode> tonic::server::UnaryService<super::Empty> for GetVersionSvc<T> {
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        type Response = super::StringValue;

                        fn call(&mut self, request: tonic::Request<super::Empty>) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { inner.get_version(request).await };
                            Box::pin(fut)
                        }
                    }
                    let inner = self.inner.clone();
                    let fut = async move {
                        let interceptor = inner.1.clone();
                        let inner = inner.0;
                        let method = GetVersionSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = if let Some(interceptor) = interceptor {
                            tonic::server::Grpc::with_interceptor(codec, interceptor)
                        } else {
                            tonic::server::Grpc::new(codec)
                        };
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                },
                "/tari.rpc.BaseNode/GetTokensInCirculation" => {
                    #[allow(non_camel_case_types)]
                    struct GetTokensInCirculationSvc<T: BaseNode>(pub Arc<T>);
                    impl<T: BaseNode> tonic::server::ServerStreamingService<super::GetBlocksRequest> for GetTokensInCirculationSvc<T> {
                        type Future = BoxFuture<tonic::Response<Self::ResponseStream>, tonic::Status>;
                        type Response = super::ValueAtHeightResponse;
                        type ResponseStream = T::GetTokensInCirculationStream;

                        fn call(&mut self, request: tonic::Request<super::GetBlocksRequest>) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { inner.get_tokens_in_circulation(request).await };
                            Box::pin(fut)
                        }
                    }
                    let inner = self.inner.clone();
                    let fut = async move {
                        let interceptor = inner.1;
                        let inner = inner.0;
                        let method = GetTokensInCirculationSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = if let Some(interceptor) = interceptor {
                            tonic::server::Grpc::with_interceptor(codec, interceptor)
                        } else {
                            tonic::server::Grpc::new(codec)
                        };
                        let res = grpc.server_streaming(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                },
                "/tari.rpc.BaseNode/GetNetworkDifficulty" => {
                    #[allow(non_camel_case_types)]
                    struct GetNetworkDifficultySvc<T: BaseNode>(pub Arc<T>);
                    impl<T: BaseNode> tonic::server::ServerStreamingService<super::HeightRequest> for GetNetworkDifficultySvc<T> {
                        type Future = BoxFuture<tonic::Response<Self::ResponseStream>, tonic::Status>;
                        type Response = super::NetworkDifficultyResponse;
                        type ResponseStream = T::GetNetworkDifficultyStream;

                        fn call(&mut self, request: tonic::Request<super::HeightRequest>) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { inner.get_network_difficulty(request).await };
                            Box::pin(fut)
                        }
                    }
                    let inner = self.inner.clone();
                    let fut = async move {
                        let interceptor = inner.1;
                        let inner = inner.0;
                        let method = GetNetworkDifficultySvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = if let Some(interceptor) = interceptor {
                            tonic::server::Grpc::with_interceptor(codec, interceptor)
                        } else {
                            tonic::server::Grpc::new(codec)
                        };
                        let res = grpc.server_streaming(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                },
                "/tari.rpc.BaseNode/GetNewBlockTemplate" => {
                    #[allow(non_camel_case_types)]
                    struct GetNewBlockTemplateSvc<T: BaseNode>(pub Arc<T>);
                    impl<T: BaseNode> tonic::server::UnaryService<super::PowAlgo> for GetNewBlockTemplateSvc<T> {
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        type Response = super::NewBlockTemplateResponse;

                        fn call(&mut self, request: tonic::Request<super::PowAlgo>) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { inner.get_new_block_template(request).await };
                            Box::pin(fut)
                        }
                    }
                    let inner = self.inner.clone();
                    let fut = async move {
                        let interceptor = inner.1.clone();
                        let inner = inner.0;
                        let method = GetNewBlockTemplateSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = if let Some(interceptor) = interceptor {
                            tonic::server::Grpc::with_interceptor(codec, interceptor)
                        } else {
                            tonic::server::Grpc::new(codec)
                        };
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                },
                "/tari.rpc.BaseNode/GetNewBlock" => {
                    #[allow(non_camel_case_types)]
                    struct GetNewBlockSvc<T: BaseNode>(pub Arc<T>);
                    impl<T: BaseNode> tonic::server::UnaryService<super::NewBlockTemplate> for GetNewBlockSvc<T> {
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        type Response = super::GetNewBlockResult;

                        fn call(&mut self, request: tonic::Request<super::NewBlockTemplate>) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { inner.get_new_block(request).await };
                            Box::pin(fut)
                        }
                    }
                    let inner = self.inner.clone();
                    let fut = async move {
                        let interceptor = inner.1.clone();
                        let inner = inner.0;
                        let method = GetNewBlockSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = if let Some(interceptor) = interceptor {
                            tonic::server::Grpc::with_interceptor(codec, interceptor)
                        } else {
                            tonic::server::Grpc::new(codec)
                        };
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                },
                "/tari.rpc.BaseNode/SubmitBlock" => {
                    #[allow(non_camel_case_types)]
                    struct SubmitBlockSvc<T: BaseNode>(pub Arc<T>);
                    impl<T: BaseNode> tonic::server::UnaryService<super::Block> for SubmitBlockSvc<T> {
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        type Response = super::Empty;

                        fn call(&mut self, request: tonic::Request<super::Block>) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { inner.submit_block(request).await };
                            Box::pin(fut)
                        }
                    }
                    let inner = self.inner.clone();
                    let fut = async move {
                        let interceptor = inner.1.clone();
                        let inner = inner.0;
                        let method = SubmitBlockSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = if let Some(interceptor) = interceptor {
                            tonic::server::Grpc::with_interceptor(codec, interceptor)
                        } else {
                            tonic::server::Grpc::new(codec)
                        };
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                },
                "/tari.rpc.BaseNode/GetTipInfo" => {
                    #[allow(non_camel_case_types)]
                    struct GetTipInfoSvc<T: BaseNode>(pub Arc<T>);
                    impl<T: BaseNode> tonic::server::UnaryService<super::Empty> for GetTipInfoSvc<T> {
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        type Response = super::TipInfoResponse;

                        fn call(&mut self, request: tonic::Request<super::Empty>) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { inner.get_tip_info(request).await };
                            Box::pin(fut)
                        }
                    }
                    let inner = self.inner.clone();
                    let fut = async move {
                        let interceptor = inner.1.clone();
                        let inner = inner.0;
                        let method = GetTipInfoSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = if let Some(interceptor) = interceptor {
                            tonic::server::Grpc::with_interceptor(codec, interceptor)
                        } else {
                            tonic::server::Grpc::new(codec)
                        };
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                },
                _ => Box::pin(async move {
                    Ok(http::Response::builder()
                        .status(200)
                        .header("grpc-status", "12")
                        .body(tonic::body::BoxBody::empty())
                        .unwrap())
                }),
            }
        }
    }
    impl<T: BaseNode> Clone for BaseNodeServer<T> {
        fn clone(&self) -> Self {
            let inner = self.inner.clone();
            Self { inner }
        }
    }
    impl<T: BaseNode> Clone for _Inner<T> {
        fn clone(&self) -> Self {
            Self(self.0.clone(), self.1.clone())
        }
    }
    impl<T: std::fmt::Debug> std::fmt::Debug for _Inner<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self.0)
        }
    }
    impl<T: BaseNode> tonic::transport::NamedService for BaseNodeServer<T> {
        const NAME: &'static str = "tari.rpc.BaseNode";
    }
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetCoinbaseRequest {
    #[prost(uint64, tag = "1")]
    pub reward: u64,
    #[prost(uint64, tag = "2")]
    pub fee: u64,
    #[prost(uint64, tag = "3")]
    pub height: u64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetCoinbaseResponse {
    #[prost(message, optional, tag = "1")]
    pub transaction: ::std::option::Option<Transaction>,
}
#[doc = r" Generated client implementations."]
pub mod wallet_client {
    #![allow(unused_variables, dead_code, missing_docs)]
    use tonic::codegen::*;
    #[doc = " The gRPC interface for interacting with the wallet."]
    pub struct WalletClient<T> {
        inner: tonic::client::Grpc<T>,
    }
    impl WalletClient<tonic::transport::Channel> {
        #[doc = r" Attempt to create a new client by connecting to a given endpoint."]
        pub async fn connect<D>(dst: D) -> Result<Self, tonic::transport::Error>
        where
            D: std::convert::TryInto<tonic::transport::Endpoint>,
            D::Error: Into<StdError>,
        {
            let conn = tonic::transport::Endpoint::new(dst)?.connect().await?;
            Ok(Self::new(conn))
        }
    }
    impl<T> WalletClient<T>
    where
        T: tonic::client::GrpcService<tonic::body::BoxBody>,
        T::ResponseBody: Body + HttpBody + Send + 'static,
        T::Error: Into<StdError>,
        <T::ResponseBody as HttpBody>::Error: Into<StdError> + Send,
    {
        pub fn new(inner: T) -> Self {
            let inner = tonic::client::Grpc::new(inner);
            Self { inner }
        }

        pub fn with_interceptor(inner: T, interceptor: impl Into<tonic::Interceptor>) -> Self {
            let inner = tonic::client::Grpc::with_interceptor(inner, interceptor);
            Self { inner }
        }

        #[doc = " This returns a coinbase transaction"]
        pub async fn get_coinbase(
            &mut self,
            request: impl tonic::IntoRequest<super::GetCoinbaseRequest>,
        ) -> Result<tonic::Response<super::GetCoinbaseResponse>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(tonic::Code::Unknown, format!("Service was not ready: {}", e.into()))
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/tari.rpc.Wallet/GetCoinbase");
            self.inner.unary(request.into_request(), path, codec).await
        }
    }
    impl<T: Clone> Clone for WalletClient<T> {
        fn clone(&self) -> Self {
            Self {
                inner: self.inner.clone(),
            }
        }
    }
    impl<T> std::fmt::Debug for WalletClient<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "WalletClient {{ ... }}")
        }
    }
}
#[doc = r" Generated server implementations."]
pub mod wallet_server {
    #![allow(unused_variables, dead_code, missing_docs)]
    use tonic::codegen::*;
    #[doc = "Generated trait containing gRPC methods that should be implemented for use with WalletServer."]
    #[async_trait]
    pub trait Wallet: Send + Sync + 'static {
        #[doc = " This returns a coinbase transaction"]
        async fn get_coinbase(
            &self,
            request: tonic::Request<super::GetCoinbaseRequest>,
        ) -> Result<tonic::Response<super::GetCoinbaseResponse>, tonic::Status>;
    }
    #[doc = " The gRPC interface for interacting with the wallet."]
    #[derive(Debug)]
    #[doc(hidden)]
    pub struct WalletServer<T: Wallet> {
        inner: _Inner<T>,
    }
    struct _Inner<T>(Arc<T>, Option<tonic::Interceptor>);
    impl<T: Wallet> WalletServer<T> {
        pub fn new(inner: T) -> Self {
            let inner = Arc::new(inner);
            let inner = _Inner(inner, None);
            Self { inner }
        }

        pub fn with_interceptor(inner: T, interceptor: impl Into<tonic::Interceptor>) -> Self {
            let inner = Arc::new(inner);
            let inner = _Inner(inner, Some(interceptor.into()));
            Self { inner }
        }
    }
    impl<T, B> Service<http::Request<B>> for WalletServer<T>
    where
        T: Wallet,
        B: HttpBody + Send + Sync + 'static,
        B::Error: Into<StdError> + Send + 'static,
    {
        type Error = Never;
        type Future = BoxFuture<Self::Response, Self::Error>;
        type Response = http::Response<tonic::body::BoxBody>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: http::Request<B>) -> Self::Future {
            let inner = self.inner.clone();
            match req.uri().path() {
                "/tari.rpc.Wallet/GetCoinbase" => {
                    #[allow(non_camel_case_types)]
                    struct GetCoinbaseSvc<T: Wallet>(pub Arc<T>);
                    impl<T: Wallet> tonic::server::UnaryService<super::GetCoinbaseRequest> for GetCoinbaseSvc<T> {
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        type Response = super::GetCoinbaseResponse;

                        fn call(&mut self, request: tonic::Request<super::GetCoinbaseRequest>) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { inner.get_coinbase(request).await };
                            Box::pin(fut)
                        }
                    }
                    let inner = self.inner.clone();
                    let fut = async move {
                        let interceptor = inner.1.clone();
                        let inner = inner.0;
                        let method = GetCoinbaseSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = if let Some(interceptor) = interceptor {
                            tonic::server::Grpc::with_interceptor(codec, interceptor)
                        } else {
                            tonic::server::Grpc::new(codec)
                        };
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                },
                _ => Box::pin(async move {
                    Ok(http::Response::builder()
                        .status(200)
                        .header("grpc-status", "12")
                        .body(tonic::body::BoxBody::empty())
                        .unwrap())
                }),
            }
        }
    }
    impl<T: Wallet> Clone for WalletServer<T> {
        fn clone(&self) -> Self {
            let inner = self.inner.clone();
            Self { inner }
        }
    }
    impl<T: Wallet> Clone for _Inner<T> {
        fn clone(&self) -> Self {
            Self(self.0.clone(), self.1.clone())
        }
    }
    impl<T: std::fmt::Debug> std::fmt::Debug for _Inner<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self.0)
        }
    }
    impl<T: Wallet> tonic::transport::NamedService for WalletServer<T> {
        const NAME: &'static str = "tari.rpc.Wallet";
    }
}
