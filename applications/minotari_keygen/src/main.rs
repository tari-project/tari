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

mod cli;

use std::process;

use clap::Parser;
use log::error;
use tari_common::{exit_codes::ExitError, initialize_logging, load_configuration};
use tari_common_types::types::PublicKey;
use tari_key_manager::cipher_seed::CipherSeed;
use tari_key_manager::key_manager::KeyManager;
use tari_key_manager::key_manager_service::{KeyDigest, KeyManagerInterface};
use tari_key_manager::mnemonic::{Mnemonic, MnemonicLanguage};
use crate::cli::Cli;
use tari_utilities::hex::Hex;
use tari_common::configuration::Network;
use tari_common_types::tari_address::TariAddress;
use tari_core::transactions::key_manager::{create_memory_db_key_manager, create_memory_db_key_manager_from_seed, MemoryDbKeyManager, SecretTransactionKeyManagerInterface, TransactionKeyManagerInterface, TransactionKeyManagerWrapper};

use tari_crypto::keys::PublicKey as PublicKeyTrait;

const LOG_TARGET: &str = "tari::keygen";
const KEY_MANAGER_COMMS_SECRET_KEY_BRANCH_KEY: &str = "comms";

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    match main_inner().await {
        Ok(_) => process::exit(0),
        Err(err) => {
            eprintln!("{}", err);
            process::exit(500)
        },
    }
    Ok(())
}
async fn main_inner() -> Result<(), anyhow::Error> {
    let cli = Cli::parse();

    let cfg = load_configuration(cli.common.config_path(), false, true, &cli)?;
    initialize_logging(
        &cli.common.log_config_path("keygen"),
        &cli.common.get_base_path(),
        include_str!("../log4rs_sample.yml"),
    )?;
    let seed = CipherSeed::new();

    let seed_words = seed.to_mnemonic(MnemonicLanguage::English, None)?;
    for i in 0..seed_words.len() {
        println!("{}: {}", i + 1, seed_words.get_word(i)?);
    }
    let comms_key_manager = KeyManager::<PublicKey, KeyDigest>::from(
        seed.clone(),
        KEY_MANAGER_COMMS_SECRET_KEY_BRANCH_KEY.to_string(),
        0,
    );
    let comms_key = comms_key_manager.derive_key(0)?.key;
    let comms_pub_key = PublicKey::from_secret_key(&comms_key);
    let network = Network::default();

    let tx_key_manager =create_memory_db_key_manager_from_seed(seed.clone(), 64);
    let view_key = tx_key_manager.get_view_key_id().await?;
    let view_key_pub = tx_key_manager.get_public_key_at_key_id(&view_key).await?;
    let tari_address =
        TariAddress::new_dual_address_with_default_features(view_key_pub.clone(), comms_pub_key.clone(), network);

    println!("Tari Address: {}", tari_address.to_hex());
    println!("Comms secret: {}", comms_key.to_hex());
    println!("Comms key: {}", comms_pub_key.to_hex());
    println!("View key: {}", view_key_pub.to_hex());
    println!("View key secret: {}", tx_key_manager.get_private_key(&view_key).await?.to_hex());
    Ok(())
}
