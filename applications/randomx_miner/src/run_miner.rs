//  Copyright 2024. The Tari Project
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

use std::{
    cmp::max,
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use log::{debug, info, warn};
use randomx_rs::{RandomXCache, RandomXDataset, RandomXFlag};
use reqwest::Client;
use tari_common::{load_configuration, DefaultConfigLoader};
use tari_core::proof_of_work::{
    randomx_factory::{RandomXFactory, RandomXVMInstance},
    Difficulty,
};
use tokio::sync::RwLock;

use crate::{
    cli::Cli,
    config::RandomXMinerConfig,
    error::{ConfigError, Error, MiningError},
    json_rpc::{get_block_count::get_block_count, get_block_template::get_block_template},
};

pub const LOG_TARGET: &str = "minotari::randomx_miner::main";

type SafeRandomXCache = Arc<RwLock<HashMap<Vec<u8>, RandomXCache>>>;
type SafeRandomXDataset = Arc<RwLock<HashMap<Vec<u8>, RandomXDataset>>>;

pub async fn start_miner(cli: Cli) -> Result<(), Error> {
    let config_path = cli.common.config_path();
    let cfg = load_configuration(config_path.as_path(), true, cli.non_interactive_mode, &cli)?;
    let mut config = RandomXMinerConfig::load_from(&cfg).expect("Failed to load config");
    config.set_base_path(cli.common.get_base_path());

    let node_address = config.monero_base_node_address.ok_or(ConfigError::MissingBaseNode)?;
    info!(target: LOG_TARGET, "Using Monero node address: {}", node_address);

    let monero_wallet_address = config
        .monero_wallet_address
        .ok_or(ConfigError::MissingMoneroWalletAddress)?;
    info!(target: LOG_TARGET, "Mining to Monero wallet address: {}", &monero_wallet_address);

    let client = Client::new();
    let mut blocks_found: u64 = 0;

    info!(target: LOG_TARGET, "Starting new mining cycle");

    let flags = RandomXFlag::get_recommended_flags() | RandomXFlag::FLAG_FULL_MEM;
    let randomx_factory = RandomXFactory::new_with_flags(2, flags);

    let caches: SafeRandomXCache = Default::default();
    let datasets: SafeRandomXDataset = Default::default();

    loop {
        thread_work(
            &client,
            &node_address,
            &monero_wallet_address,
            &randomx_factory,
            caches.clone(),
            datasets.clone(),
        )
        .await?;
    }
}

async fn thread_work(
    client: &Client,
    node_address: &String,
    monero_wallet_address: &String,
    randomx_factory: &RandomXFactory,
    caches: SafeRandomXCache,
    datasets: SafeRandomXDataset,
) -> Result<(), MiningError> {
    let flags = randomx_factory.get_flags()?;
    let current_height = get_block_count(client, node_address).await?;
    let block_template = get_block_template(client, node_address, monero_wallet_address).await?;
    let blockhashing_bytes = hex::decode(block_template.blockhashing_blob)?;

    let key = hex::decode(&block_template.prev_hash)?;

    debug!(target: LOG_TARGET, "Initializing cache");
    let read_lock = caches.read().await;
    let (flags, cache) = match read_lock.get(&key) {
        Some(cache) => (flags, cache.clone()),
        None => match RandomXCache::new(flags, &key) {
            Ok(cache) => (flags, cache),
            Err(err) => {
                drop(read_lock);
                warn!(
                    target: LOG_TARGET,
                    "Error initializing RandomX cache with flags {:?}. {:?}. Fallback to default flags", flags, err
                );
                // This is informed by how RandomX falls back on any cache allocation failure
                // https://github.com/xmrig/xmrig/blob/02b2b87bb685ab83b132267aa3c2de0766f16b8b/src/crypto/rx/RxCache.cpp#L88
                let flags = RandomXFlag::FLAG_DEFAULT;
                let cache = RandomXCache::new(flags, &key)?;
                caches.write().await.insert(key.to_vec(), cache.clone());
                (flags, cache)
            },
        },
    };
    debug!(target: LOG_TARGET, "Initializing dataset");
    let read_lock = datasets.read().await;
    let dataset = match read_lock.get(&key) {
        Some(dataset) => dataset.clone(),
        None => {
            drop(read_lock);
            let d = RandomXDataset::new(RandomXFlag::FLAG_DEFAULT, cache.clone(), 0)?;
            datasets.write().await.insert(key.to_vec(), d.clone());
            d
        },
    };

    let vm = randomx_factory.create(&key, Some(cache), Some(dataset))?;
    let mut count = 0u32;
    let start_time = Instant::now();
    let mut last_check_time = start_time;
    let mut max_difficulty_reached = 0;
    debug!(target: LOG_TARGET, "Mining now");
    loop {
        let (difficulty, hash) = mining_cycle(blockhashing_bytes.clone(), count, vm.clone()).await?;
        count += 1;

        // Check the hash rate every second
        let now = Instant::now();
        let elapsed_since_last_check = now.duration_since(last_check_time);

        if elapsed_since_last_check >= Duration::from_secs(10) {
            let total_elapsed_time = now.duration_since(start_time).as_secs_f64();
            let hash_rate = count as f64 / total_elapsed_time;

            println!("Hash Rate: {:.2} H/s", hash_rate);

            last_check_time = now; // Reset the last check time
        }

        unsafe {
            if difficulty.as_u64() > max_difficulty_reached {
                max_difficulty_reached = difficulty.as_u64();
                println!("New max difficulty reached: {}", max_difficulty_reached);
            }
        }

        if difficulty.as_u64() >= block_template.difficulty {
            println!("Valid block found!");
            return Ok(());
        }
    }
}

async fn mining_cycle(
    mut blockhashing_bytes: Vec<u8>,
    count: u32,
    vm: RandomXVMInstance,
) -> Result<(Difficulty, Vec<u8>), MiningError> {
    let nonce_position = 38;
    blockhashing_bytes[nonce_position..nonce_position + 4].copy_from_slice(&count.to_le_bytes());

    let hash = vm.calculate_hash(&blockhashing_bytes)?;
    // Check last byte of hash and see if it's over difficulty
    let difficulty = Difficulty::little_endian_difficulty(&hash)?;

    Ok((difficulty, hash))
}
