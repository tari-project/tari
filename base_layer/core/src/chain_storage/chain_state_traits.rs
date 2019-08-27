// This is a TODO checklist of functionality for the database
pub trait RewindableBlockChain {
    /// Set the horizon beyond which we cannot provide detailed blockchain information anymore.
    /// A value of zero indicates that no pruning should be carried out at all. That is, this state should act as a
    /// archival node.
    /// An error can be returned if:
    ///   * The new horizon is further back than the current horizon.
    fn set_pruning_horizon(&mut self, new_pruning_horizon: usize) -> Result<(), ChainStorageError>;

    /// Rewind the blockchain state to the block height given.
    ///
    /// An error is returned if
    /// * The block height is in the future
    /// * The block height is before pruning horizon
    fn rewind_to_height(&mut self, height: u64) -> Result<(), ChainStorageError>;
}



/// Add a block to the longest chain. This function does some basic checks to maintain the chain integrity, but
/// does not perform a full block validation (this should have been done by this point).
///
/// On completion, this function will have
///   * Checked that the previous block builds on the longest chain.
///       * If not - does it have a parent on the chain && is its total accumulated PoW higher than current total?
///       * If yes, add to orphan pool. And check for a re-org.
///   * That the total accumulated work has increased.
///   * Mark all inputs in the block as spent.
///   * Updated the database metadata
///
/// An error is returned if:
///   * the block has already been added
///   * any of the inputs were not in the UTXO set or were marked as spent already
///
/// If an error does occur while writing the new block parts, all changes are reverted before returning.
pub fn add_block(&mut self, block: Block) -> Result<(), ChainStorageError> {
    let parent_hash = &block.header.prev_hash;
    self.con
    let (header, inputs, outputs, kernels) = block.dissolve();
    let mut txn = DbTransaction::new();
    txn.insert_header(header);
    txn.spend_inputs(&inputs);
    outputs.into_iter().for_each(|utxo| txn.insert_utxo(utxo));
    kernels.into_iter().for_each(|k| txn.insert_kernel(k));
    self.commit(txn)
}