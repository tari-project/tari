// Copyright 2019 The Tari Project
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

use chrono::Duration;
use merklemountainrange::mmr::*;
use rand::OsRng;
use tari_core::{block::*, blockheader::*, pow::*, transaction::*, types::*};
use tari_crypto::{keys::SecretKey, ristretto::*};
use tari_utilities::hash::Hashable;

/// This is used to represent a block chain in memory for testing purposes
pub struct SimpleBlockChain {
    blocks: Vec<Block>,
    headers: MerkleMountainRange<BlockHeader, SignatureHash>,
    utxos: MerkleMountainRange<TransactionInput, SignatureHash>,
    kernels: MerkleMountainRange<TransactionKernel, SignatureHash>,
}
impl SimpleBlockChain {
    /// This will create a new test block_chain
    pub fn new(amount: u32) -> SimpleBlockChain {
        let mut chain = SimpleBlockChain::default();

        let mut rng = OsRng::new().unwrap();
        // create gen block
        let priv_key = PrivateKey::random(&mut rng);
        let block = BlockBuilder::new().create_coinbase(priv_key).build_with_pow();
        chain.processes_new_block(block);

        // lets mine some more blocks
        for _i in 1..(amount) {
            let priv_key = PrivateKey::random(&mut rng);
            let header = chain.generate_new_header();
            let block = BlockBuilder::new()
                .with_header(header)
                .create_coinbase(priv_key)
                .build_with_pow();
            chain.processes_new_block(block);
        }
        chain
    }

    pub fn add(&mut self, amount: u32) {
        let mut rng = OsRng::new().unwrap();
        for _i in 0..(amount) {
            let priv_key = PrivateKey::random(&mut rng);
            let header = self.generate_new_header();
            let block = BlockBuilder::new()
                .with_header(header)
                .create_coinbase(priv_key)
                .build_with_pow();
            self.processes_new_block(block);
        }
    }

    fn processes_new_block(&mut self, block: Block) {
        self.headers
            .push(block.header.clone())
            .expect("failed to add header to test chain");
        self.utxos
            .append(block.body.inputs.clone())
            .expect("failed to add inputs to test chain");
        self.kernels
            .append(block.body.kernels.clone())
            .expect("failed to add kernels to test chain");
        self.blocks.push(block);
    }

    fn generate_new_header(&self) -> BlockHeader {
        let counter = self.blocks.len() - 1;
        let mut hash = [0; 32];
        hash.copy_from_slice(&self.blocks[counter].header.hash());
        BlockHeader {
            version: BLOCKCHAIN_VERSION,
            height: self.blocks[counter].header.height + 1,
            prev_hash: hash,
            timestamp: self.blocks[counter]
                .header
                .timestamp
                .clone()
                .checked_add_signed(Duration::minutes(1))
                .unwrap(),
            output_mmr: [0; 32],
            range_proof_mmr: [0; 32],
            kernel_mmr: [0; 32],
            total_kernel_offset: RistrettoSecretKey::from(0),
            pow: ProofOfWork {},
        }
    }
}

impl Default for SimpleBlockChain {
    fn default() -> Self {
        SimpleBlockChain {
            blocks: Vec::new(),
            headers: MerkleMountainRange::new(),
            utxos: MerkleMountainRange::new(),
            kernels: MerkleMountainRange::new(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn create_simple_block_chain() {
        let mut chain = SimpleBlockChain::new(5);
        assert_eq!(chain.blocks.len(), 5);
        chain.add(5);
        assert_eq!(chain.blocks.len(), 10);

        assert_eq!(chain.blocks[0].header.height, 0);
        for i in 1..10 {
            let mut hash = [0; 32];
            hash.copy_from_slice(&chain.blocks[i - 1].header.hash());
            assert_eq!(chain.blocks[i].header.prev_hash, hash);
            assert_eq!(chain.blocks[i].header.height, i as u64);
        }
    }

}
