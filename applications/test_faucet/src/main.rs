// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    fs::File,
    io,
    io::{stdout, BufRead, Write},
};

use serde::Serialize;
use tari_common_types::types::{Commitment, PrivateKey, RangeProof};
use tari_core::{
    covenants::Covenant,
    transactions::{
        tari_amount::{MicroTari, T},
        test_helpers,
        test_helpers::generate_keys,
        transaction_components::{
            EncryptedValue,
            KernelFeatures,
            OutputFeatures,
            TransactionKernel,
            TransactionOutput,
            TransactionOutputVersion,
        },
        CryptoFactories,
    },
};
use tari_crypto::{commitment::HomomorphicCommitmentFactory, range_proof::RangeProofService, tari_utilities::hex::Hex};
use tari_script::{script, TariScript};
use tari_utilities::ByteArray;
use tokio::{sync::mpsc, task};

const NUM_KEYS: usize = 4000;

#[derive(Serialize)]
struct Key {
    key: String,
    value: u64,
    commitment: String,
    proof: String,
}

/// UTXO generation is pretty slow (esp range proofs), so we'll use async threads to speed things up.
/// We'll use blocking thread tasks to do the CPU intensive utxo generation, and then push the results
/// through a channel where a file-writer is waiting to persist the results to disk.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let num_keys: usize = std::env::args()
        .skip(1)
        .take(1)
        .fold(NUM_KEYS, |def, v| v.parse::<usize>().unwrap_or(def));

    // Create a channel to give the file writer output as the utxos are generated
    let (tx, rx) = mpsc::channel::<(TransactionOutput, PrivateKey, MicroTari)>(500);

    println!("Setting up output");
    let write_fut = task::spawn(write_keys(rx));

    println!("Generating {} UTXOs..", num_keys);
    let factories = CryptoFactories::default();
    let values = Values;
    let features = UTXOFeatures;
    // Use Rust's awesome Iterator trait to produce a sequence of values and output features.
    for (value, feature) in values.take(num_keys).zip(features.take(num_keys)) {
        let fc = factories.clone();
        let txc = tx.clone();
        // Notice the `spawn(.. spawn_blocking)` nested call here. If we don't do this, we're basically queuing up
        // blocking tasks, `await`ing them to finish, and then queueing up the next one. In effect we're running things
        // synchronously.
        // What this construction says is: Queue up this task, and move on. "this task" (the spawning of the blocking
        // task and awaiting its result) is not run immediately, but pushed to the scheduler to execute when it's
        // ready. Now, we will use all the available threads for generating the keys (and the output should print
        // "Go!" before, or right the beginning of any key generation output.
        task::spawn(async move {
            let result = task::spawn_blocking(move || {
                let script = script!(Nop);
                let (utxo, key, _) = create_utxo(value, &fc, feature, script, Covenant::default());
                print!(".");
                let _result = stdout().flush();
                (utxo, key, value)
            })
            .await
            .expect("Could not create key");
            let _result = txc.send(result).await;
        });
    }
    println!("Go!");
    // Explicitly drop the tx side here, so that rx will end its input.
    drop(tx);

    let _res = write_fut.await;
    Ok(())
}

async fn write_keys(mut rx: mpsc::Receiver<(TransactionOutput, PrivateKey, MicroTari)>) {
    let mut utxo_file = File::create("utxos.json").expect("Could not create utxos.json");
    let mut key_file = File::create("keys.json").expect("Could not create keys.json");
    let mut written: u64 = 0;
    let mut key_sum = PrivateKey::default();
    // The receiver channel will patiently await results until the tx is dropped.
    while let Some((utxo, key, value)) = rx.recv().await {
        key_sum = key_sum + key.clone();
        let key = Key {
            key: key.to_hex(),
            value: u64::from(value),
            commitment: utxo.commitment.to_hex(),
            proof: utxo.proof.to_hex(),
        };
        let key_str = format!("{}\n", serde_json::to_string(&key).unwrap());
        let _result = key_file.write_all(key_str.as_bytes());

        let utxo_s = serde_json::to_string(&utxo).unwrap();
        match utxo_file.write_all(format!("{}\n", utxo_s).as_bytes()) {
            Ok(_) => {
                written += 1;
                if written % 50 == 0 {
                    println!("{} outputs written", written);
                }
            },
            Err(e) => println!("{}", e),
        }
    }
    let (pk, sig) = test_helpers::create_random_signature_from_s_key(key_sum, 0.into(), 0, KernelFeatures::empty());
    let excess = Commitment::from_public_key(&pk);
    let kernel =
        TransactionKernel::new_current_version(KernelFeatures::empty(), MicroTari::from(0), 0, excess, sig, None);
    let kernel = serde_json::to_string(&kernel).unwrap();
    let _result = utxo_file.write_all(format!("{}\n", kernel).as_bytes());

    // Test the UTXO file
    println!("\n\nTesting the UTXO file...\n\n");
    drop(utxo_file);
    let file = File::open("utxos.json").unwrap();
    let mut counter = 1;
    for line in io::BufReader::new(file).lines() {
        if counter < 4001 {
            let utxo: TransactionOutput = serde_json::from_str(&*line.unwrap()).unwrap();
            match utxo.verify_range_proof(&CryptoFactories::default().range_proof) {
                Ok(_) => {},
                Err(e) => {
                    println!("proof error! {}: {}", counter, e);
                },
            }
            match utxo.verify_metadata_signature() {
                Ok(_) => {},
                Err(e) => {
                    println!("metadata_signature error! {}: {}", counter, e);
                },
            }
        } else {
            let kernel: TransactionKernel = serde_json::from_str(&*line.unwrap()).unwrap();
            match kernel.verify_signature() {
                Ok(_) => {},
                Err(e) => {
                    println!("kernel_signature error! {}: {}", counter, e);
                },
            }
            println!();
            break;
        }
        counter += 1;
    }

    println!("Done.");
}

struct Values;

impl Iterator for Values {
    type Item = MicroTari;

    fn next(&mut self) -> Option<Self::Item> {
        Some(10 * T)
    }
}

struct UTXOFeatures;

impl Iterator for UTXOFeatures {
    type Item = OutputFeatures;

    fn next(&mut self) -> Option<Self::Item> {
        let f = OutputFeatures::default();
        Some(f)
    }
}

/// Create a new UTXO for the specified value and return the output and spending key
fn create_utxo(
    value: MicroTari,
    factories: &CryptoFactories,
    features: OutputFeatures,
    script: TariScript,
    covenant: Covenant,
) -> (TransactionOutput, PrivateKey, PrivateKey) {
    let keys = generate_keys();
    let offset_keys = generate_keys();
    let commitment = factories.commitment.commit_value(&keys.k, value.into());
    let proof = factories.range_proof.construct_proof(&keys.k, value.into()).unwrap();
    if !factories.range_proof.verify(&proof, &commitment) {
        panic!("Range proof does not verify");
    };
    let encrypted_value = EncryptedValue::default();
    let minimum_value_promise = MicroTari::zero();
    let metadata_sig = TransactionOutput::create_final_metadata_signature(
        TransactionOutputVersion::get_current_version(),
        value,
        &keys.k,
        &script,
        &features,
        &offset_keys.k,
        &covenant,
        &encrypted_value,
        minimum_value_promise,
    )
    .unwrap();

    let utxo = TransactionOutput::new_current_version(
        features,
        commitment,
        RangeProof::from_bytes(&proof).unwrap(),
        script,
        offset_keys.pk,
        metadata_sig,
        covenant,
        encrypted_value,
        minimum_value_promise,
    );
    utxo.verify_range_proof(&CryptoFactories::default().range_proof)
        .unwrap();
    (utxo, keys.k, offset_keys.k)
}
