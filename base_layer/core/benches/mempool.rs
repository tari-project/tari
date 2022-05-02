//  Copyright 2022. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

#[cfg(not(feature = "benches"))]
mod benches {
    pub fn main() {
        println!("Enable the `benches` feature to run benches");
    }
}

#[cfg(feature = "benches")]
mod benches {
    use std::sync::Arc;

    use criterion::{criterion_group, BatchSize, Criterion};
    use futures::future::try_join_all;
    use tari_common::configuration::Network;
    use tari_core::{
        consensus::ConsensusManager,
        mempool::{Mempool, MempoolConfig},
        test_helpers::blockchain::create_new_blockchain,
        transactions::{
            tari_amount::{uT, T},
            transaction_components::{OutputFeatures, Transaction, MAX_TRANSACTION_OUTPUTS},
            CryptoFactories,
        },
        tx,
        validation::transaction_validators::{
            MempoolValidator,
            TxConsensusValidator,
            TxInputAndMaturityValidator,
            TxInternalConsistencyValidator,
        },
    };
    use tokio::{runtime::Runtime, task};

    async fn generate_transactions(
        num_txs: usize,
        num_inputs: usize,
        num_outputs: usize,
        features: OutputFeatures,
    ) -> Vec<Arc<Transaction>> {
        let tasks = (0..num_txs)
            .map(|_| {
                let features = features.clone();
                task::spawn_blocking(move || {
                    let (tx, _, _) = tx!(T, fee: uT, inputs: num_inputs, outputs: num_outputs, features: features);
                    Arc::new(tx)
                })
            })
            .collect::<Vec<_>>();

        try_join_all(tasks).await.unwrap()
    }

    pub fn mempool_perf_test(c: &mut Criterion) {
        let runtime = Runtime::new().unwrap();
        let config = MempoolConfig::default();
        let rules = ConsensusManager::builder(Network::LocalNet).build();
        let db = create_new_blockchain();

        let mempool_validator = MempoolValidator::new(vec![
            Box::new(TxInternalConsistencyValidator::new(
                CryptoFactories::default(),
                false,
                db.clone(),
            )),
            Box::new(TxInputAndMaturityValidator::new(db.clone())),
            Box::new(TxConsensusValidator::new(db)),
        ]);
        let mempool = Mempool::new(config, rules, Box::new(mempool_validator));
        const NUM_TXNS: usize = 100;
        // Pre-generate a bunch of transactions
        eprintln!(
            "Generating {} transactions with {} total inputs and {} total outputs...",
            NUM_TXNS,
            NUM_TXNS * 1000,
            NUM_TXNS * MAX_TRANSACTION_OUTPUTS
        );
        let transactions = runtime.block_on(generate_transactions(
            NUM_TXNS,
            1000,
            MAX_TRANSACTION_OUTPUTS,
            OutputFeatures::for_minting(Default::default(), Default::default(), vec![1, 2, 3], None),
        ));
        c.bench_function("Mempool Insert", move |b| {
            let mut offset = 0;
            b.iter_batched(
                || {
                    let batch = transactions[offset..offset + 10].to_vec();
                    offset = (offset + 10) % NUM_TXNS;
                    batch
                },
                |txns| runtime.block_on(async { mempool.insert_all(txns).await.unwrap() }),
                BatchSize::SmallInput,
            );
        });
    }

    criterion_group!(
        name = mempool_perf;
        config = Criterion::default().sample_size(10);
        targets = mempool_perf_test
    );

    pub fn main() {
        mempool_perf();
        criterion::Criterion::default().configure_from_args().final_summary();
    }
}

fn main() {
    benches::main();
}
