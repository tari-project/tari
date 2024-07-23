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

use std::{
    fmt,
    fmt::{Display, Formatter},
};

use borsh::{BorshDeserialize, BorshSerialize};
use log::*;
use serde::{Deserialize, Serialize};
use tari_common_types::types::{FixedHash, PrivateKey};
use thiserror::Error;

use crate::{
    blocks::BlockHeader,
    consensus::ConsensusConstants,
    proof_of_work::ProofOfWork,
    transactions::{
        aggregated_body::AggregateBody,
        tari_amount::MicroMinotari,
        transaction_components::{
            KernelFeatures,
            OutputType,
            Transaction,
            TransactionError,
            TransactionInput,
            TransactionKernel,
            TransactionOutput,
        },
        CryptoFactories,
    },
};

#[derive(Clone, Debug, Error)]
pub enum BlockValidationError {
    #[error("A transaction in the block failed to validate: `{0}`")]
    TransactionError(#[from] TransactionError),
    #[error("Mismatched {kind} MMR roots")]
    MismatchedMmrRoots { kind: &'static str },
    #[error("MMR size for {mmr_tree} does not match. Expected: {expected}, received: {actual}")]
    MismatchedMmrSize {
        mmr_tree: String,
        expected: u64,
        actual: u64,
    },
}

/// A Minotari block. Blocks are linked together into a blockchain.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Block {
    /// The BlockHeader contains all the metadata for the block, including proof of work, a link to the previous block
    /// and the transaction kernels.
    pub header: BlockHeader,
    /// The components of the block or transaction. The same struct can be used for either, since in Mimblewimble,
    /// blocks consist of inputs, outputs and kernels, rather than transactions.
    pub body: AggregateBody,
}

impl Block {
    pub fn new(header: BlockHeader, body: AggregateBody) -> Self {
        Self { header, body }
    }

    pub fn version(&self) -> u16 {
        self.header.version
    }

    /// This function will calculate the total fees contained in a block
    pub fn calculate_fees(&self) -> MicroMinotari {
        self.body.kernels().iter().fold(0.into(), |sum, x| sum + x.fee)
    }

    /// Run through the outputs of the block and check that
    /// 1. There is exactly ONE coinbase output
    /// 2. The output's maturity is correctly set
    /// 3. The amount is correct.
    pub fn check_coinbase_output(
        &self,
        reward: MicroMinotari,
        consensus_constants: &ConsensusConstants,
        factories: &CryptoFactories,
    ) -> Result<(), BlockValidationError> {
        self.body.check_coinbase_output(
            reward,
            consensus_constants.coinbase_min_maturity(),
            factories,
            self.header.height,
        )?;
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
        let (inputs, outputs, kernels) = self.body.dissolve();
        (self.header, inputs, outputs, kernels)
    }

    /// Destroys the block and returns the pieces of the block: header, body
    pub fn into_header_body(self) -> (BlockHeader, AggregateBody) {
        (self.header, self.body)
    }

    /// Return a cloned version of this block with the TransactionInputs in their compact form
    pub fn to_compact(&self) -> Self {
        Self {
            header: self.header.clone(),
            body: self.body.to_compact(),
        }
    }

    /// The block hash is just the header hash, since the inputs, outputs and range proofs are captured by their
    /// respective MMR roots in the header itself.
    pub fn hash(&self) -> FixedHash {
        self.header.hash()
    }
}

impl Display for Block {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        writeln!(f, "----------------- Block -----------------")?;
        writeln!(f, "--- Header ---")?;
        writeln!(f, "Hash: {}", self.header.hash())?;
        writeln!(f, "{}", self.header)?;
        writeln!(f, "---  Body  ---")?;
        writeln!(f, "{}", self.body)
    }
}

pub struct BlockBuilder {
    header: BlockHeader,
    inputs: Vec<TransactionInput>,
    outputs: Vec<TransactionOutput>,
    kernels: Vec<TransactionKernel>,
}

impl BlockBuilder {
    pub fn new(blockchain_version: u16) -> BlockBuilder {
        BlockBuilder {
            header: BlockHeader::new(blockchain_version),
            inputs: Vec::new(),
            outputs: Vec::new(),
            kernels: Vec::new(),
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

    /// This function adds the provided transaction outputs to the block WITHOUT updating output_mmr_size in the header
    pub fn add_outputs(mut self, mut outputs: Vec<TransactionOutput>) -> Self {
        self.outputs.append(&mut outputs);
        self
    }

    /// This function adds the provided transaction kernels to the block WITHOUT updating kernel_mmr_size in the header
    pub fn add_kernels(mut self, mut kernels: Vec<TransactionKernel>) -> Self {
        self.kernels.append(&mut kernels);
        self
    }

    /// This functions adds the provided transactions to the block, modifying the header MMR counts and offsets
    pub fn with_transactions(mut self, txs: Vec<Transaction>) -> Self {
        for tx in txs {
            self = self.add_transaction(tx)
        }
        self
    }

    /// This functions adds the provided transaction to the block, modifying the header MMR counts and offsets
    pub fn add_transaction(mut self, tx: Transaction) -> Self {
        let (inputs, outputs, kernels) = tx.body.dissolve();
        self = self.add_inputs(inputs);
        self.header.output_smt_size += outputs.len() as u64;
        self = self.add_outputs(outputs);
        self.header.kernel_mmr_size += kernels.len() as u64;
        self = self.add_kernels(kernels);
        self.header.total_kernel_offset = self.header.total_kernel_offset + tx.offset;
        self.header.total_script_offset = self.header.total_script_offset + tx.script_offset;
        self
    }

    /// This will add the given coinbase UTXO to the block
    pub fn with_coinbase_utxo(mut self, coinbase_utxo: TransactionOutput, coinbase_kernel: TransactionKernel) -> Self {
        self.kernels.push(coinbase_kernel);
        self.outputs.push(coinbase_utxo);
        self
    }

    /// Add the provided ProofOfWork metadata to the block
    pub fn with_pow(mut self, pow: ProofOfWork) -> Self {
        self.header.pow = pow;
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
}

//---------------------------------- NewBlock --------------------------------------------//
pub struct NewBlock {
    /// The block header.
    pub header: BlockHeader,
    /// Coinbase kernel of the block
    pub coinbase_kernels: Vec<TransactionKernel>,
    /// Coinbase output of the block
    pub coinbase_outputs: Vec<TransactionOutput>,
    /// The scalar `s` component of the kernel excess signatures of the transactions contained in the block.
    pub kernel_excess_sigs: Vec<PrivateKey>,
}

impl From<&Block> for NewBlock {
    fn from(block: &Block) -> Self {
        let coinbase_kernels = block
            .body
            .kernels()
            .clone()
            .into_iter()
            .filter(|k| k.features.contains(KernelFeatures::COINBASE_KERNEL))
            .collect();
        let coinbase_outputs = block
            .body
            .outputs()
            .clone()
            .into_iter()
            .filter(|o| o.features.output_type == OutputType::Coinbase)
            .collect();

        Self {
            header: block.header.clone(),
            coinbase_kernels,
            coinbase_outputs,
            kernel_excess_sigs: block
                .body
                .kernels()
                .iter()
                .filter(|k| !k.features.contains(KernelFeatures::COINBASE_KERNEL))
                .map(|kernel| kernel.excess_sig.get_signature().clone())
                .collect(),
        }
    }
}
