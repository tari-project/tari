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
    blockheader::BlockHeader,
    emission::*,
    tari_amount::*,
    transaction::*,
    transaction_protocol::{build_challenge, TransactionMetadata},
    types::{
        BlindingFactor,
        Commitment,
        CommitmentFactory,
        PrivateKey,
        RangeProof,
        RangeProofService,
        Signature,
        COMMITMENT_FACTORY,
        PROVER,
    },
};
use serde::{Deserialize, Serialize};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    keys::{PublicKey, SecretKey},
    range_proof::RangeProofService as RPS,
    ristretto::pedersen::PedersenCommitment,
};
use tari_utilities::ByteArray;

//----------------------------------------         Blocks         ----------------------------------------------------//

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
    pub fn check_internal_consistency(&self) -> Result<(), TransactionError> {
        let offset = &self.header.total_kernel_offset;
        self.body
            .validate_internal_consistency(offset, &PROVER, &COMMITMENT_FACTORY)?;
        self.check_pow()
    }

    pub fn check_pow(&self) -> Result<(), TransactionError> {
        Ok(())
    }

    /// This function will calculate the pow for the block and fill out the pow header field
    pub fn calculate_pow(&mut self) -> Result<(), TransactionError> {
        // todo
        Ok(())
    }
}

// todo this probably need to move somewhere else
/// This function will create the correct amount for the coinbase given the block height, it will provide the answer in
/// µTari (micro Tari)
pub fn calculate_coinbase(block_height: u64) -> MicroTari {
    // todo fill this in properly as a function and not a constant
    let schedule = EmissionSchedule::new(MicroTari::from(10_000_000), 0.999, MicroTari::from(100));
    schedule.block_reward(block_height)
}

//----------------------------------------     AggregateBody      ----------------------------------------------------//

/// The components of the block or transaction. The same struct can be used for either, since in Mimblewimble,
/// cut-through means that blocks and transactions have the same structure.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AggregateBody {
    sorted: bool,
    /// List of inputs spent by the transaction.
    pub inputs: Vec<TransactionInput>,
    /// List of outputs the transaction produces.
    pub outputs: Vec<TransactionOutput>,
    /// Kernels contain the excesses and their signatures for transaction
    pub kernels: Vec<TransactionKernel>,
}

impl AggregateBody {
    /// Create an empty aggregate body
    pub fn empty() -> AggregateBody {
        AggregateBody {
            sorted: false,
            inputs: vec![],
            outputs: vec![],
            kernels: vec![],
        }
    }

    /// Create a new aggregate body from provided inputs, outputs and kernels
    pub fn new(
        inputs: Vec<TransactionInput>,
        outputs: Vec<TransactionOutput>,
        kernels: Vec<TransactionKernel>,
    ) -> AggregateBody
    {
        AggregateBody {
            sorted: false,
            inputs,
            outputs,
            kernels,
        }
    }

    /// Add an input to the existing aggregate body
    pub fn add_input(&mut self, input: TransactionInput) {
        self.inputs.push(input);
        self.sorted = false;
    }

    /// Add a series of inputs to the existing aggregate body
    pub fn add_inputs(&mut self, inputs: &mut Vec<TransactionInput>) {
        self.inputs.append(inputs);
        self.sorted = false;
    }

    /// Add an output to the existing aggregate body
    pub fn add_output(&mut self, output: TransactionOutput) {
        self.outputs.push(output);
        self.sorted = false;
    }

    /// Add an output to the existing aggregate body
    pub fn add_outputs(&mut self, outputs: &mut Vec<TransactionOutput>) {
        self.outputs.append(outputs);
        self.sorted = false;
    }

    /// Add a kernel to the existing aggregate body
    pub fn add_kernel(&mut self, kernel: TransactionKernel) {
        self.kernels.push(kernel);
    }

    /// Set the kernel of the aggregate body, replacing any previous kernels
    pub fn set_kernel(&mut self, kernel: TransactionKernel) {
        self.kernels = vec![kernel];
    }

    /// Sort the component lists of the aggregate body
    pub fn sort(&mut self) {
        if self.sorted {
            return;
        }
        self.inputs.sort();
        self.outputs.sort();
        self.kernels.sort();
        self.sorted = true;
    }

    /// Verify the signatures in all kernels contained in this aggregate body. Clients must provide an offset that
    /// will be added to the public key used in the signature verification.
    pub fn verify_kernel_signatures(&self) -> Result<(), TransactionError> {
        for kernel in self.kernels.iter() {
            kernel.verify_signature()?;
        }
        Ok(())
    }

    pub fn get_total_fee(&self) -> MicroTari {
        let mut fee = MicroTari::from(0);
        for kernel in &self.kernels {
            fee += kernel.fee;
        }
        fee
    }

    /// Validate this transaction by checking the following:
    /// 1. The sum of inputs, outputs and fees equal the (public excess value + offset)
    /// 1. The signature signs the canonical message with the private excess
    /// 1. Range proofs of the outputs are valid
    ///
    /// This function does NOT check that inputs come from the UTXO set
    pub fn validate_internal_consistency(
        &self,
        offset: &BlindingFactor,
        prover: &RangeProofService,
        factory: &CommitmentFactory,
    ) -> Result<(), TransactionError>
    {
        self.verify_kernel_signatures()?;
        self.validate_kernel_sum(offset, factory)?;
        self.validate_range_proofs(prover)
    }

    /// Calculate the sum of the inputs and outputs including fees
    fn sum_commitments(&self, fees: u64, factory: &CommitmentFactory) -> Commitment {
        let fee_commitment = factory.commit_value(&PrivateKey::default(), fees);
        let sum_inputs = &self.inputs.iter().map(|i| &i.commitment).sum::<Commitment>();
        let sum_outputs = &self.outputs.iter().map(|o| &o.commitment).sum::<Commitment>();
        &(sum_outputs - sum_inputs) + &fee_commitment
    }

    /// Calculate the sum of the kernels, taking into account the provided offset, and their constituent fees
    fn sum_kernels(&self, offset: &BlindingFactor) -> KernelSum {
        let public_offset = PublicKey::from_secret_key(offset);
        let offset_commitment = PedersenCommitment::from_public_key(&public_offset);
        // Sum all kernel excesses and fees
        self.kernels.iter().fold(
            KernelSum {
                fees: MicroTari(0),
                sum: offset_commitment,
            },
            |acc, val| KernelSum {
                fees: &acc.fees + &val.fee,
                sum: &acc.sum + &val.excess,
            },
        )
    }

    /// Confirm that the (sum of the outputs) - (sum of inputs) = Kernel excess
    fn validate_kernel_sum(
        &self,
        offset: &BlindingFactor,
        factory: &CommitmentFactory,
    ) -> Result<(), TransactionError>
    {
        let kernel_sum = self.sum_kernels(offset);
        let sum_io = self.sum_commitments(kernel_sum.fees.into(), factory);

        if kernel_sum.sum != sum_io {
            return Err(TransactionError::ValidationError(
                "Sum of inputs and outputs did not equal sum of kernels with fees".into(),
            ));
        }

        Ok(())
    }

    fn validate_range_proofs(&self, range_proof_service: &RangeProofService) -> Result<(), TransactionError> {
        for o in &self.outputs {
            if !o.verify_range_proof(&range_proof_service)? {
                return Err(TransactionError::ValidationError(
                    "Range proof could not be verified".into(),
                ));
            }
        }
        Ok(())
    }
}

/// This will strip away the offset of the transaction returning a pure aggregate body
impl From<Transaction> for AggregateBody {
    fn from(transaction: Transaction) -> Self {
        transaction.body
    }
}

#[derive(Default)]
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
            header: BlockBuilder::gen_blank_header(),
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

    /// This function will create a coinbase from the provided secret key. The coinbase will be added to the outputs and
    /// kernels.
    pub fn create_coinbase(mut self, key: PrivateKey) -> Self {
        let mut rng = rand::OsRng::new().unwrap();
        // build output
        let amount = self.total_fee + calculate_coinbase(self.header.height);
        let v = PrivateKey::from(u64::from(amount));
        let commitment = COMMITMENT_FACTORY.commit(&key, &v);
        let rr = PROVER.construct_proof(&v, amount.into()).unwrap();
        let output = TransactionOutput::new(
            OutputFeatures::COINBASE_OUTPUT,
            commitment,
            RangeProof::from_bytes(&rr).unwrap(),
        );

        // create kernel
        let tx_meta = TransactionMetadata {
            fee: 0.into(),
            lock_height: 0,
        };
        let r = PrivateKey::random(&mut rng);
        let e = build_challenge(&PublicKey::from_secret_key(&r), &tx_meta);
        let s = Signature::sign(key.clone(), r, &e).unwrap();
        let excess = COMMITMENT_FACTORY.commit_value(&key, 0);
        let kernel = KernelBuilder::new()
            .with_features(KernelFeatures::COINBASE_KERNEL)
            .with_fee(0.into())
            .with_lock_height(0)
            .with_excess(&excess)
            .with_signature(&s)
            .build()
            .unwrap();
        self.kernels.push(kernel);
        self.outputs.push(output);
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

    /// This will finish construction of the block, do proof of work and create the block
    pub fn build_with_pow(self) -> Block {
        let mut block = Block {
            header: self.header,
            body: AggregateBody::new(self.inputs, self.outputs, self.kernels),
        };
        block.body.sort();
        block
            .calculate_pow()
            .expect("failure to calculate the block proof of work");
        block
    }

    /// This is just a wrapper function to return a blank header
    fn gen_blank_header() -> BlockHeader {
        BlockHeader::default()
    }
}

/// This struct holds the result of calculating the sum of the kernels in a Transaction
/// and returns the summed commitments and the total fees
pub struct KernelSum {
    pub sum: Commitment,
    pub fees: MicroTari,
}

//----------------------------------------         Tests          ----------------------------------------------------//
