// Copyright 2020. The Tari Project
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

use std::{
    fs,
    io::{self, Write},
    path::Path,
    sync::Arc,
};

use anyhow::anyhow;
use log::*;

use tari_common::{configuration::Network, exit_codes::ExitCodes, DatabaseType, GlobalConfig};
use tari_core::{
    chain_storage::{
        async_db::AsyncBlockchainDb,
        create_lmdb_database,
        create_recovery_lmdb_database,
        BlockchainBackend,
        BlockchainDatabase,
        BlockchainDatabaseConfig,
        Validators,
    },
    consensus::ConsensusManager,
    proof_of_work::randomx_factory::RandomXFactory,
    transactions::CryptoFactories,
    validation::{
        block_validators::{BodyOnlyValidator, OrphanBlockValidator},
        header_validator::HeaderValidator,
        mocks::MockValidator,
        DifficultyCalculator,
    },
};

pub const LOG_TARGET: &str = "base_node::app";

pub fn initiate_recover_db(node_config: &GlobalConfig) -> Result<(), ExitCodes> {
    // create recovery db
    match &node_config.db_type {
        DatabaseType::LMDB(p) => {
            let _backend = create_recovery_lmdb_database(&p).map_err(|err| {
                error!(target: LOG_TARGET, "{}", err);
                ExitCodes::UnknownError(err.to_string())
            })?;
        },
        _ => {
            const MSG: &str = "Recovery mode is only available for LMDB";
            error!(target: LOG_TARGET, "{}", MSG);
            return Err(ExitCodes::UnknownError(MSG.to_string()));
        },
    };
    Ok(())
}

pub async fn run_recovery(node_config: &GlobalConfig) -> Result<(), anyhow::Error> {
    println!("Starting recovery mode");
    let (temp_db, main_db) = match &node_config.db_type {
        DatabaseType::LMDB(p) => {
            let backend = create_lmdb_database(&p, node_config.db_config.clone()).map_err(|e| {
                error!(target: LOG_TARGET, "Error opening db: {}", e);
                anyhow!("Could not open DB: {}", e)
            })?;
            let new_path = Path::new(&p).join("temp_recovery");

            let temp = create_lmdb_database(&new_path, node_config.db_config.clone()).map_err(|e| {
                error!(target: LOG_TARGET, "Error opening recovery db: {}", e);
                anyhow!("Could not open recovery DB: {}", e)
            })?;
            (temp, backend)
        },
        _ => {
            error!(target: LOG_TARGET, "Recovery mode is only available for LMDB");
            return Err(anyhow!("Recovery mode is only available for LMDB"));
        },
    };
    let rules = ConsensusManager::builder(node_config.network).build();
    let factories = CryptoFactories::default();
    let randomx_factory = RandomXFactory::new(node_config.max_randomx_vms);
    let validators = Validators::new(
        BodyOnlyValidator::default(),
        HeaderValidator::new(rules.clone()),
        OrphanBlockValidator::new(
            rules.clone(),
            node_config.base_node_bypass_range_proof_verification,
            factories.clone(),
        ),
    );
    let db_config = BlockchainDatabaseConfig {
        orphan_storage_capacity: node_config.orphan_storage_capacity,
        pruning_horizon: node_config.pruning_horizon,
        pruning_interval: node_config.pruned_mode_cleanup_interval,
    };
    let db = BlockchainDatabase::new(
        main_db,
        rules.clone(),
        validators,
        db_config,
        DifficultyCalculator::new(rules, randomx_factory),
        true,
    )?;
    do_recovery(db.into(), temp_db).await?;

    info!(
        target: LOG_TARGET,
        "Node has completed recovery mode, it will try to cleanup the db"
    );
    match &node_config.db_type {
        DatabaseType::LMDB(p) => {
            let new_path = Path::new(p).join("temp_recovery");
            fs::remove_dir_all(&new_path).map_err(|e| {
                error!(target: LOG_TARGET, "Error opening recovery db: {}", e);
                anyhow!("Could not open recovery DB: {}", e)
            })
        },
        _ => {
            error!(target: LOG_TARGET, "Recovery mode is only available for LMDB");
            Ok(())
        },
    }
}

// Function to handle the recovery attempt of the db
async fn do_recovery<D: BlockchainBackend + 'static>(
    db: AsyncBlockchainDb<D>,
    source_backend: D,
) -> Result<(), anyhow::Error> {
    // We dont care about the values, here, so we just use mock validators, and a mainnet CM.
    let rules = ConsensusManager::builder(Network::LocalNet).build();
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    let source_database = BlockchainDatabase::new(
        source_backend,
        rules.clone(),
        validators,
        BlockchainDatabaseConfig::default(),
        DifficultyCalculator::new(rules, Default::default()),
        false,
    )?;
    let max_height = source_database
        .get_chain_metadata()
        .map_err(|e| anyhow!("Could not get max chain height: {}", e))?
        .height_of_longest_chain();
    // we start at height 1
    let mut counter = 1;
    print!("Starting recovery at height: ");
    loop {
        print!("{}", counter);
        io::stdout().flush().unwrap();
        trace!(target: LOG_TARGET, "Asking for block with height: {}", counter);
        let block = source_database
            .fetch_block(counter)
            .map_err(|e| anyhow!("Could not get block from recovery db: {}", e))?
            .try_into_block()?;
        trace!(target: LOG_TARGET, "Adding block: {}", block);
        db.add_block(Arc::new(block))
            .await
            .map_err(|e| anyhow!("Stopped recovery at height {}, reason: {}", counter, e))?;
        if counter >= max_height {
            info!(target: LOG_TARGET, "Done with recovery, chain height {}", counter);
            break;
        }
        print!("\x1B[{}D\x1B[K", counter.to_string().len());
        counter += 1;
    }
    Ok(())
}
