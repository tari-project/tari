use rand::{self, Rng};
use serde::Serialize;
use tari_core::{
    tari_utilities::hex::Hex,
    transactions::{
        helpers,
        tari_amount::{MicroTari, T},
        transaction::{OutputFeatures, TransactionOutput},
        types::{CryptoFactories, PrivateKey},
    },
};

use std::{fs::File, io::Write};
use tokio::{sync::mpsc, task};
use tari_core::transactions::transaction::UnblindedOutput;

const NUM_KEYS: usize = 10;

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
#[tokio::main(core_threads = 2, max_threads = 10)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let num_keys: usize = std::env::args()
        .skip(1)
        .take(1)
        .fold(NUM_KEYS, |def, v| v.parse::<usize>().unwrap_or(def));

    // Create a channel to give the file writer output as the utxos are generated
    let (tx, rx) = mpsc::channel::<UnblindedOutput>(500);

    println!("Setting up output");
    let write_fut = task::spawn(write_keys(rx));

    println!("Generating {} UTXOs..", num_keys);
    let factories = CryptoFactories::default();
    let values = Values;
    let features = UTXOFeatures;
    // Use Rust's awesome Iterator trait to produce a sequence of values and output features.
    for (value, feature) in values.take(num_keys).zip(features.take(num_keys)) {
        let fc = factories.clone();
        let mut txc = tx.clone();
        // Notice the `spawn(.. spawn_blocking)` nested call here. If we don't do this, we're basically queuing up
        // blocking tasks, `await`ing them to finish, and then queueing up the next one. In effect we're running things
        // synchronously.
        // What this construction says is: Queue up this task, and move on. "this task" (the spawning of the blocking
        // task and awaiting its result) is not run immediately, but pushed to the scheduler to execute when it's
        // ready. Now, we will use all the available threads for generating the keys (and the output should print
        // "Go!" before, or right the beginning of any key generation output.
        task::spawn(async move {
            let result = task::spawn_blocking(move || {
                let utxo = helpers::create_utxo(value, &fc, Some(feature), None).unwrap();
                print!(".");
                utxo
            })
            .await
            .expect("Could not create key");
            let _ = txc.send(result).await;
        });
    }
    println!("Go!");
    // Explicitly drop the tx side here, so that rx will end its input.
    drop(tx);

    let _res = write_fut.await;
    Ok(())
}

async fn write_keys(mut rx: mpsc::Receiver<UnblindedOutput>) {
    let mut utxo_file = File::create("utxos.json").expect("Could not create utxos.json");
    let mut key_file = File::create("keys.json").expect("Could not create keys.json");
    let mut written: u64 = 0;
    let factories = CryptoFactories::default();
    // The receiver channel will patiently await results until the tx is dropped.
    while let Some(utxo) = rx.recv().await {
        let output = utxo.as_transaction_output(&factories).unwrap();
        let key = Key {
            key: utxo.blinding_factor().to_hex(),
            value: u64::from(utxo.value()),
            commitment: utxo.commitment().to_hex(),
            proof: output.proof().to_hex(),
        };
        let key_str = format!("{}\n", serde_json::to_string(&key).unwrap());
        let _ = key_file.write_all(key_str.as_bytes());
        let utxo_s = serde_json::to_string(&output).unwrap();
        match utxo_file.write_all(format!("{}\n", utxo_s).as_bytes()) {
            Ok(_) => {
                written += 1;
                if written % 50 == 0 {
                    println!("{} outputs written", written);
                }
            },
            Err(e) => println!("{}", e.to_string()),
        }
    }
    println!("Done.");
}

struct Values;

impl Iterator for Values {
    type Item = MicroTari;

    fn next(&mut self) -> Option<Self::Item> {
        let mut rng = rand::rngs::OsRng;
        let extra = rng.gen_range(0, 25) * 10_000_000;
        Some(5000 * T + MicroTari(extra))
    }
}

struct UTXOFeatures;

impl Iterator for UTXOFeatures {
    type Item = OutputFeatures;

    fn next(&mut self) -> Option<Self::Item> {
        let f = OutputFeatures::with_maturity(0);
        Some(f)
    }
}
