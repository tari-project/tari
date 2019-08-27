// A temporary holding place for stuff from blockchain_state that's not strictly state related

let schedule = EmissionSchedule::new(MicroTari::from(10_000_000), 0.999, MicroTari::from(100)); // ToDo ensure these amounts are correct

/// This function will create the correct amount for the coinbase given the block height, it will provide the answer
/// in µTari (micro Tari)
fn calculate_coinbase(&self, block_height: u64) -> MicroTari {
    self.schedule.block_reward(block_height)
}




// add the genesis block
fn add_genesis_block(&mut self) {
    let gen_block = get_genesis_block();
    for output in gen_block.body.outputs {
        self.utxos.push(output.into()).expect("genesis block failed")
    }
    self.kernels
        .append(gen_block.body.kernels)
        .expect("genesis block failed");
    self.headers.push(gen_block.header).expect("genesis block failed");

    self.check_point_state().expect("genesis block failed");
}

/// This function is just a wrapper function to call checkpoint on all the MMR's
fn check_mmr_states(&mut self) -> Result<(), StateError> {
    // if this unwrap fails there is something weird wrong as the headers did not get added.
    let last_header = self.headers.get_last_added_object()
        .expect("Expected a header in blockchain state but found none");

    // Compute output merkle root per consensus rules
    let utxo_hash = self.calculate_utxo_hash();

    if (last_header.output_mr != utxo_hash[..] ||
        (last_header.kernel_mr != self.kernels.get_merkle_root()[..])) ||
        (last_header.range_proof_mr != self.rangeproofs.get_merkle_root()[..])
    {
        return Err(StateError::HeaderStateMismatch); // TODO return a specific error which mmr state failed?
    }
    Ok(())
}

/// This function will validate the block in terms of the current state.
pub fn validate_new_block(&self, new_block: &Block) -> Result<(), StateError> {
    new_block
        .check_internal_consistency(self.calculate_coinbase(new_block.header.height))
        .map_err(|e| StateError::InvalidBlock(e))?;
    // we assume that we have atleast in block in the headers mmr even if this is the genesis one
    if self.headers.get_last_added_object().unwrap().hash() != new_block.header.prev_hash {
        return Err(StateError::OrphanBlock);
    }
    Ok(())
}