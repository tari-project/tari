use crate::{
    pow::ProofOfWork,
    transaction::{BlindingFactor, TransactionInput, TransactionKernel, TransactionOutput},
};
use chrono::{DateTime, Utc};

type BlockHash = [u8; 32];

/// A Tari block. Blocks are linked together into a blockchain.
pub struct Block {
    pub header: BlockHeader,
    pub body: AggregateBody,
}

/// The BlockHeader contains all the metadata for the block, including proof of work, a link to the previous block
/// and the transaction kernels.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockHeader {
    /// Version of the block
    pub version: u16,
    /// Height of this block since the genesis block (height 0)
    pub height: u64,
    /// Hash of the block previous to this in the chain.
    pub prev_hash: BlockHash,
    /// Timestamp at which the block was built.
    pub timestamp: DateTime<Utc>,
    /// Total accumulated sum of kernel offsets since genesis block. We can derive the kernel offset sum for *this*
    /// block from the total kernel offset of the previous block header.
    pub total_kernel_offset: BlindingFactor,
    /// Proof of work summary
    pub pow: ProofOfWork,
}

/// The components of the block or transaction. The same struct can be used for either, since in Mimblewimble,
/// cut-through means that blocks and transactions have the same structure.
pub struct AggregateBody {
    /// List of inputs spent by the transaction.
    pub inputs: Vec<TransactionInput>,
    /// List of outputs the transaction produces.
    pub outputs: Vec<TransactionOutput>,
    /// Kernels contain the excesses and their signatures for transaction
    pub kernels: Vec<TransactionKernel>,
}
