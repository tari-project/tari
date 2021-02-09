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

use anyhow::anyhow;
use log::*;
use std::{
    fs,
    io::{self, Write},
    path::Path,
    sync::Arc,
};
use tari_app_utilities::utilities::ExitCodes;
use tari_common::{DatabaseType, GlobalConfig};
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
    consensus::{ConsensusManagerBuilder, Network as NetworkType},
    proof_of_work::randomx_factory::{RandomXConfig, RandomXFactory},
    transactions::types::CryptoFactories,
    validation::{
        block_validators::{BodyOnlyValidator, OrphanBlockValidator},
        header_validator::HeaderValidator,
        mocks::MockValidator,
    },
};

pub const LOG_TARGET: &str = "base_node::app";

pub fn initiate_recover_db(node_config: &GlobalConfig) -> Result<(), ExitCodes> {
    // create recovery db
    match &node_config.db_type {
        DatabaseType::LMDB(p) => {
            let _backend = create_recovery_lmdb_database(&p).map_err(|err| {
                error!(target: LOG_TARGET, "{}", err);
                ExitCodes::UnknownError
            })?;
        },
        _ => {
            error!(target: LOG_TARGET, "Recovery mode is only available for LMDB");
            return Err(ExitCodes::UnknownError);
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
    let rules = ConsensusManagerBuilder::new(node_config.network.into()).build();
    let factories = CryptoFactories::default();
    let randomx_factory = RandomXFactory::new(RandomXConfig::default(), node_config.max_randomx_vms);
    let validators = Validators::new(
        BodyOnlyValidator::default(),
        HeaderValidator::new(rules.clone(), randomx_factory),
        OrphanBlockValidator::new(rules.clone(), factories.clone()),
    );
    let db_config = BlockchainDatabaseConfig {
        orphan_storage_capacity: node_config.orphan_storage_capacity,
        pruning_horizon: node_config.pruning_horizon,
        pruning_interval: node_config.pruned_mode_cleanup_interval,
    };
    let db = BlockchainDatabase::new(main_db, &rules, validators, db_config, true)?;
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
    temp_db: D,
) -> Result<(), anyhow::Error>
{
    // We dont care about the values, here, so we just use mock validators, and a mainnet CM.
    let rules = ConsensusManagerBuilder::new(NetworkType::LocalNet).build();
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    let temp_db_backend =
        BlockchainDatabase::new(temp_db, &rules, validators, BlockchainDatabaseConfig::default(), false)?;
    let max_height = temp_db_backend
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
        let block = temp_db_backend
            .fetch_block(counter)
            .map_err(|e| anyhow!("Could not get block from recovery db: {}", e))?
            .try_into_block()?;
        trace!(target: LOG_TARGET, "Adding block: {}", block);
        db.add_block(Arc::new(block))
            .await
            .map_err(|e| anyhow!("Stopped recovery at height {}, reason: {}", counter, e))?;
        counter += 1;
        if counter > max_height {
            info!(target: LOG_TARGET, "Done with recovery, chain height {}", counter - 1);
            break;
        }
        print!("\x1B[{}D\x1B[K", (counter + 1).to_string().chars().count());
    }
    Ok(())
}
