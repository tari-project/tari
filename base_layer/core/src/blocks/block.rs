// Copyright 2018 The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
//
// Portions of this file were originally copyrighted (c) 2018 The Grin Developers, issued under the Apache License,
// Version 2.0, available at http://www.apache.org/licenses/LICENSE-2.0.
use crate::{
    blocks::{aggregated_body::AggregateBody, blockheader::BlockHeader},
    consensus::ConsensusRules,
    proof_of_work::PowError,
    tari_amount::*,
    transaction::*,
    types::{Commitment, HashDigest, TariProofOfWork, COMMITMENT_FACTORY, PROVER},
};
use derive_error::Error;
use serde::{Deserialize, Serialize};
use tari_utilities::Hashable;

#[derive(Clone, Debug, PartialEq, Error)]
pub enum BlockValidationError {
    // A transaction in the block failed to validate
    TransactionError(TransactionError),
    // Invalid Proof of work for the block
    ProofOfWorkError(PowError),
    // Invalid kernel in block
    InvalidKernel,
    // Invalid input in block
    InvalidInput,
    // Input maturity not reached
    InputMaturity,
    // Invalid coinbase maturity in block or more than one coinbase
    InvalidCoinbase,
}

/// A Tari block. Blocks are linked together into a blockchain.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Block {
    pub header: BlockHeader,
    pub body: AggregateBody,
}

impl Block {
    /// This function will check the block to ensure that all UTXO's are validly constructed and that all signatures are
    /// valid. It does _not_ check that the inputs exist in the current UTXO set;
    /// nor does it check that the PoW is the largest accumulated PoW value.
    pub fn check_internal_consistency(&self, rules: &ConsensusRules) -> Result<(), BlockValidationError> {
        let block_reward = rules.emission_schedule().block_reward(self.header.height);
        let offset = &self.header.total_kernel_offset;
        let total_coinbase = self.calculate_coinbase_and_fees(block_reward);
        self.body
            .validate_internal_consistency(&offset, total_coinbase, &PROVER, &COMMITMENT_FACTORY)?;
        self.check_stxo_rules()?;
        self.check_utxo_rules(rules)?;
        self.check_pow()
    }

    // create a total_coinbase offset containing all fees for the validation
    fn calculate_coinbase_and_fees(&self, block_reward: MicroTari) -> MicroTari {
        let mut coinbase = block_reward;
        for kernel in &self.body.kernels {
            coinbase += kernel.fee;
        }
        coinbase
    }

    pub fn check_pow(&self) -> Result<(), BlockValidationError> {
        Ok(())
    }

    /// This function will check spent kernel rules like tx lock height etc
    pub fn check_kernel_rules(&self) -> Result<(), BlockValidationError> {
        for kernel in &self.body.kernels {
            if kernel.lock_height > self.header.height {
                return Err(BlockValidationError::InvalidKernel);
            }
        }
        Ok(())
    }

    /// This function will check all new utxo to ensure that feature flags where set
    pub fn check_utxo_rules(&self, current_rules: &ConsensusRules) -> Result<(), BlockValidationError> {
        let mut coinbase_counter = 0; // there should be exactly 1 coinbase
        for utxo in &self.body.outputs {
            if utxo.features.flags.contains(OutputFlags::COINBASE_OUTPUT) {
                coinbase_counter += 1;
                if utxo.features.maturity < (self.header.height + current_rules.coinbase_lock_height()) {
                    return Err(BlockValidationError::InvalidCoinbase);
                }
            }
        }
        if coinbase_counter != 1 {
            return Err(BlockValidationError::InvalidCoinbase);
        }
        Ok(())
    }

    /// This function will check all stxo to ensure that feature flags where followed
    pub fn check_stxo_rules(&self) -> Result<(), BlockValidationError> {
        for input in &self.body.inputs {
            if input.features.maturity > self.header.height {
                return Err(BlockValidationError::InputMaturity);
            }
        }
        Ok(())
    }

    /// Destroys the block and returns the pieces of the block: header, inputs, outputs and kernels
    pub fn dissolve(
        self,
    ) -> (
        BlockHeader,
        Vec<TransactionInput>,
        Vec<TransactionOutput>,
        Vec<TransactionKernel>,
    ) {
        (self.header, self.body.inputs, self.body.outputs, self.body.kernels)
    }
}

pub struct BlockBuilder {
    pub header: BlockHeader,
    pub inputs: Vec<TransactionInput>,
    pub outputs: Vec<TransactionOutput>,
    pub kernels: Vec<TransactionKernel>,
    pub total_fee: MicroTari,
}

impl BlockBuilder {
    pub fn new() -> BlockBuilder {
        BlockBuilder {
            header: BlockHeader::new(ConsensusRules::current().blockchain_version()),
            inputs: Vec::new(),
            outputs: Vec::new(),
            kernels: Vec::new(),
            total_fee: MicroTari::from(0),
        }
    }

    /// This function adds a header to the block
    pub fn with_header(mut self, header: BlockHeader) -> Self {
        self.header = header;
        self
    }

    /// This function adds the provided transaction inputs to the block
    pub fn add_inputs(mut self, mut inputs: Vec<TransactionInput>) -> Self {
        self.inputs.append(&mut inputs);
        self
    }

    /// This function adds the provided transaction outputs to the block
    pub fn add_outputs(mut self, mut outputs: Vec<TransactionOutput>) -> Self {
        self.outputs.append(&mut outputs);
        self
    }

    /// This function adds the provided transaction kernels to the block
    pub fn add_kernels(mut self, mut kernels: Vec<TransactionKernel>) -> Self {
        for kernel in &kernels {
            self.total_fee += kernel.fee;
        }
        self.kernels.append(&mut kernels);
        self
    }

    /// This functions add the provided transactions to the block
    pub fn with_transactions(mut self, txs: Vec<Transaction>) -> Self {
        let iter = txs.into_iter();
        for tx in iter {
            self = self.add_inputs(tx.body.inputs);
            self = self.add_outputs(tx.body.outputs);
            self = self.add_kernels(tx.body.kernels);
            self.header.total_kernel_offset = self.header.total_kernel_offset + tx.offset;
        }
        self
    }

    /// This functions add the provided transactions to the block
    pub fn add_transaction(mut self, tx: Transaction) -> Self {
        self = self.add_inputs(tx.body.inputs);
        self = self.add_outputs(tx.body.outputs);
        self = self.add_kernels(tx.body.kernels);
        self.header.total_kernel_offset = &self.header.total_kernel_offset + &tx.offset;
        self
    }

    /// This will add the given coinbase UTXO to the block
    pub fn with_coinbase_utxo(mut self, coinbase_utxo: TransactionOutput, coinbase_kernel: TransactionKernel) -> Self {
        self.kernels.push(coinbase_kernel);
        self.outputs.push(coinbase_utxo);
        self
    }

    /// This will finish construction of the block and create the block
    pub fn build(self) -> Block {
        let mut block = Block {
            header: self.header,
            body: AggregateBody::new(self.inputs, self.outputs, self.kernels),
        };
        block.body.sort();
        block
    }

    /// Add the provided ProofOfWork to the block
    pub fn with_pow(self, _pow: TariProofOfWork) -> Self {
        // TODO
        self
    }
}

/// This struct holds the result of calculating the sum of the kernels in a Transaction
/// and returns the summed commitments and the total fees
pub struct KernelSum {
    pub sum: Commitment,
    pub fees: MicroTari,
}

impl Hashable for Block {
    /// The block hash is just the header hash, since the inputs, outputs and range proofs are captured by their
    /// respective MMR roots in the header itself.
    fn hash(&self) -> Vec<u8> {
        self.header.hash()
    }
}

//----------------------------------------         Tests          ----------------------------------------------------//
