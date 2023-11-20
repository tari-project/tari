//   Copyright 2023. The Tari Project
//
//   Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//   following conditions are met:
//
//   1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//   disclaimer.
//
//   2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//   following disclaimer in the documentation and/or other materials provided with the distribution.
//
//   3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//   products derived from this software without specific prior written permission.
//
//   THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//   INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//   DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//   SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//   SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//   WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//   USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    collections::VecDeque,
    fmt::{Debug, Formatter},
    path::PathBuf,
};

use cucumber::gherkin::{Feature, Scenario};
use indexmap::IndexMap;
use rand::rngs::OsRng;
use serde_json::Value;
use tari_chat_client::ChatClient;
use tari_common::configuration::Network;
use tari_common_types::{
    tari_address::TariAddress,
    types::{PrivateKey, PublicKey},
};
use tari_core::{
    blocks::Block,
    consensus::ConsensusManager,
    transactions::{
        key_manager::{create_memory_db_key_manager, MemoryDbKeyManager, TariKeyId},
        transaction_components::{Transaction, WalletOutput},
    },
};
use tari_crypto::keys::{PublicKey as PK, SecretKey};
use tari_key_manager::key_manager_service::{KeyId, KeyManagerInterface};
use tari_utilities::hex::Hex;
use thiserror::Error;

use crate::{
    base_node_process::BaseNodeProcess,
    get_base_dir,
    merge_mining_proxy::MergeMiningProxyProcess,
    miner::MinerProcess,
    wallet_ffi::WalletFFI,
    wallet_process::WalletProcess,
};

#[derive(Error, Debug)]
pub enum TariWorldError {
    #[error("Base node process not found: {0}")]
    BaseNodeProcessNotFound(String),
    #[error("Wallet process not found: {0}")]
    WalletProcessNotFound(String),
    #[error("FFIWallet not found: {0}")]
    FFIWalletNotFound(String),
    #[error("Miner process not found: {0}")]
    MinerProcessNotFound(String),
    #[error("Merge miner process not found: {0}")]
    MergeMinerProcessNotFound(String),
    #[error("No base node, or wallet client found: {0}")]
    ClientNotFound(String),
}

#[derive(cucumber::World)]
pub struct TariWorld {
    pub current_scenario_name: Option<String>,
    pub current_feature_name: Option<String>,
    pub current_base_dir: Option<PathBuf>,
    pub base_nodes: IndexMap<String, BaseNodeProcess>,
    pub blocks: IndexMap<String, Block>,
    pub miners: IndexMap<String, MinerProcess>,
    pub ffi_wallets: IndexMap<String, WalletFFI>,
    pub wallets: IndexMap<String, WalletProcess>,
    pub chat_clients: IndexMap<String, Box<dyn ChatClient>>,
    pub merge_mining_proxies: IndexMap<String, MergeMiningProxyProcess>,
    pub transactions: IndexMap<String, Transaction>,
    pub wallet_addresses: IndexMap<String, String>, // values are strings representing tari addresses
    pub utxos: IndexMap<String, WalletOutput>,
    pub output_hash: Option<String>,
    pub pre_image: Option<String>,
    pub wallet_connected_to_base_node: IndexMap<String, String>, // wallet -> base node,
    pub seed_nodes: Vec<String>,
    // mapping from hex string of public key of wallet client to tx_id's
    pub wallet_tx_ids: IndexMap<String, Vec<u64>>,
    pub errors: VecDeque<String>,
    // We need to store this in between steps when importing and checking the imports.
    pub last_imported_tx_ids: Vec<u64>,
    // We need to store this for the merge mining proxy steps. The checks are get and check are done on separate steps.
    pub last_merge_miner_response: Value,
    pub key_manager: MemoryDbKeyManager,
    // This will be used for all one-sided coinbase payments
    pub wallet_private_key: PrivateKey,
    // This receiver wallet address will be used for default one-sided coinbase payments
    pub default_payment_address: TariAddress,
    pub consensus_manager: ConsensusManager,
}

impl Default for TariWorld {
    fn default() -> Self {
        println!("\nWorld initialized - remove this line when called!\n");
        let wallet_private_key = PrivateKey::random(&mut OsRng);
        let default_payment_address =
            TariAddress::new(PublicKey::from_secret_key(&wallet_private_key), Network::LocalNet);
        Self {
            current_scenario_name: None,
            current_feature_name: None,
            current_base_dir: None,
            base_nodes: Default::default(),
            blocks: Default::default(),
            miners: Default::default(),
            ffi_wallets: Default::default(),
            wallets: Default::default(),
            chat_clients: Default::default(),
            merge_mining_proxies: Default::default(),
            transactions: Default::default(),
            wallet_addresses: Default::default(),
            utxos: Default::default(),
            output_hash: None,
            pre_image: None,
            wallet_connected_to_base_node: Default::default(),
            seed_nodes: vec![],
            wallet_tx_ids: Default::default(),
            errors: Default::default(),
            last_imported_tx_ids: vec![],
            last_merge_miner_response: Default::default(),
            key_manager: create_memory_db_key_manager(),
            wallet_private_key,
            default_payment_address,
            consensus_manager: ConsensusManager::builder(Network::LocalNet).build().unwrap(),
        }
    }
}

impl Debug for TariWorld {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("base_nodes", &self.base_nodes)
            .field("blocks", &self.blocks)
            .field("miners", &self.miners)
            .field("ffi_wallets", &self.ffi_wallets)
            .field("wallets", &self.wallets)
            .field("merge_mining_proxies", &self.merge_mining_proxies)
            .field("chat_clients", &self.chat_clients.keys())
            .field("transactions", &self.transactions)
            .field("wallet_addresses", &self.wallet_addresses)
            .field("utxos", &self.utxos)
            .field("output_hash", &self.output_hash)
            .field("pre_image", &self.pre_image)
            .field("wallet_connected_to_base_node", &self.wallet_connected_to_base_node)
            .field("seed_nodes", &self.seed_nodes)
            .field("wallet_tx_ids", &self.wallet_tx_ids)
            .field("errors", &self.errors)
            .field("last_imported_tx_ids", &self.last_imported_tx_ids)
            .field("last_merge_miner_response", &self.last_merge_miner_response)
            .finish()
    }
}

pub enum NodeClient {
    BaseNode(minotari_node_grpc_client::BaseNodeGrpcClient<tonic::transport::Channel>),
    Wallet(minotari_wallet_grpc_client::WalletGrpcClient<tonic::transport::Channel>),
}

impl TariWorld {
    pub async fn get_node_client<S: AsRef<str>>(
        &self,
        name: &S,
    ) -> anyhow::Result<minotari_node_grpc_client::BaseNodeGrpcClient<tonic::transport::Channel>> {
        self.get_node(name)?.get_grpc_client().await
    }

    pub async fn get_base_node_or_wallet_client<S: core::fmt::Debug + AsRef<str>>(
        &self,
        name: S,
    ) -> anyhow::Result<NodeClient> {
        match self.get_node_client(&name).await {
            Ok(client) => Ok(NodeClient::BaseNode(client)),
            Err(_) => match self.get_wallet_client(&name).await {
                Ok(wallet) => Ok(NodeClient::Wallet(wallet)),
                Err(e) => Err(TariWorldError::ClientNotFound(e.to_string()).into()),
            },
        }
    }

    pub async fn get_wallet_address<S: AsRef<str>>(&self, name: &S) -> anyhow::Result<String> {
        if let Some(address) = self.wallet_addresses.get(name.as_ref()) {
            return Ok(address.clone());
        }
        match self.get_wallet_client(name).await {
            Ok(wallet) => {
                let mut wallet = wallet;

                Ok(wallet
                    .get_address(minotari_wallet_grpc_client::grpc::Empty {})
                    .await
                    .unwrap()
                    .into_inner()
                    .address
                    .to_hex())
            },
            Err(_) => {
                let ffi_wallet = self.get_ffi_wallet(name).unwrap();

                Ok(ffi_wallet.get_address().address().get_as_hex())
            },
        }
    }

    #[allow(dead_code)]
    pub async fn get_wallet_client<S: AsRef<str>>(
        &self,
        name: &S,
    ) -> anyhow::Result<minotari_wallet_grpc_client::WalletGrpcClient<tonic::transport::Channel>> {
        self.get_wallet(name)?.get_grpc_client().await
    }

    pub fn get_node<S: AsRef<str>>(&self, node_name: &S) -> anyhow::Result<&BaseNodeProcess> {
        Ok(self
            .base_nodes
            .get(node_name.as_ref())
            .ok_or_else(|| TariWorldError::BaseNodeProcessNotFound(node_name.as_ref().to_string()))?)
    }

    pub fn get_wallet<S: AsRef<str>>(&self, wallet_name: &S) -> anyhow::Result<&WalletProcess> {
        Ok(self
            .wallets
            .get(wallet_name.as_ref())
            .ok_or_else(|| TariWorldError::WalletProcessNotFound(wallet_name.as_ref().to_string()))?)
    }

    pub fn get_ffi_wallet<S: AsRef<str>>(&self, wallet_name: &S) -> anyhow::Result<&WalletFFI> {
        Ok(self
            .ffi_wallets
            .get(wallet_name.as_ref())
            .ok_or_else(|| TariWorldError::FFIWalletNotFound(wallet_name.as_ref().to_string()))?)
    }

    pub fn get_mut_ffi_wallet<S: AsRef<str>>(&mut self, wallet_name: &S) -> anyhow::Result<&mut WalletFFI> {
        Ok(self
            .ffi_wallets
            .get_mut(wallet_name.as_ref())
            .ok_or_else(|| TariWorldError::FFIWalletNotFound(wallet_name.as_ref().to_string()))?)
    }

    pub fn get_miner<S: AsRef<str>>(&self, miner_name: S) -> anyhow::Result<&MinerProcess> {
        Ok(self
            .miners
            .get(miner_name.as_ref())
            .ok_or_else(|| TariWorldError::MinerProcessNotFound(miner_name.as_ref().to_string()))?)
    }

    pub fn get_merge_miner<S: AsRef<str>>(&self, miner_name: S) -> anyhow::Result<&MergeMiningProxyProcess> {
        Ok(self
            .merge_mining_proxies
            .get(miner_name.as_ref())
            .ok_or_else(|| TariWorldError::MergeMinerProcessNotFound(miner_name.as_ref().to_string()))?)
    }

    pub fn get_mut_merge_miner<S: AsRef<str>>(
        &mut self,
        miner_name: S,
    ) -> anyhow::Result<&mut MergeMiningProxyProcess> {
        Ok(self
            .merge_mining_proxies
            .get_mut(miner_name.as_ref())
            .ok_or_else(|| TariWorldError::MergeMinerProcessNotFound(miner_name.as_ref().to_string()))?)
    }

    pub fn all_seed_nodes(&self) -> &[String] {
        self.seed_nodes.as_slice()
    }

    pub async fn before(&mut self, feature: &Feature, scenario: &Scenario) {
        self.current_feature_name = Some(feature.name.clone());
        self.current_scenario_name = Some(scenario.name.clone());
        self.current_base_dir = Some(get_base_dir().join(feature.name.clone()).join(scenario.name.clone()))
    }

    pub async fn after(&mut self, _scenario: &Scenario) {
        for (name, mut p) in self.chat_clients.drain(..) {
            println!("Shutting down chat client {}", name);
            p.shutdown();
        }
        for (name, mut p) in self.wallets.drain(..) {
            println!("Shutting down wallet {}", name);
            p.kill_signal.trigger();
        }
        for (name, mut p) in self.base_nodes.drain(..) {
            println!("Shutting down base node {}", name);
            // You have explicitly trigger the shutdown now because of the change to use Arc/Mutex in tari_shutdown
            p.kill_signal.trigger();
        }
    }

    pub async fn miner_node_script_key_id(&mut self) -> TariKeyId {
        match self.key_manager.import_key(self.wallet_private_key.clone()).await {
            Ok(key_id) => key_id,
            Err(_) => KeyId::Imported {
                key: PublicKey::from_secret_key(&self.wallet_private_key),
            },
        }
    }
}
