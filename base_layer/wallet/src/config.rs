// Copyright 2021. The Tari Project
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

use std::{
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use strum::EnumString;
use tari_common::{
    configuration::{serializers, Network, StringList},
    SubConfigPath,
};
use tari_common_types::{grpc_authentication::GrpcAuthentication, wallet_types::WalletType};
use tari_comms::multiaddr::Multiaddr;
use tari_p2p::P2pConfig;
use tari_utilities::SafePassword;

use crate::{
    base_node_service::config::BaseNodeServiceConfig,
    output_manager_service::config::OutputManagerServiceConfig,
    transaction_service::config::TransactionServiceConfig,
};

pub const KEY_MANAGER_COMMS_SECRET_KEY_BRANCH_KEY: &str = "comms";

fn deserialize_safe_password_option<'de, D>(deserializer: D) -> Result<Option<SafePassword>, D::Error>
where D: serde::Deserializer<'de> {
    let password: Option<String> = Deserialize::deserialize(deserializer)?;
    Ok(password.map(SafePassword::from))
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct WalletConfig {
    pub override_from: Option<String>,
    /// The p2p config settings
    pub p2p: P2pConfig,
    /// The transaction_service_config config settings
    #[serde(rename = "transactions")]
    pub transaction_service_config: TransactionServiceConfig,
    /// The output_manager_service_config config settings
    #[serde(rename = "outputs")]
    pub output_manager_service_config: OutputManagerServiceConfig,
    /// The buffer size for the publish/subscribe connector channel, connecting comms messages to the domain layer
    pub buffer_size: usize,
    /// Selected network
    pub network: Network,
    /// The base_node_service_config config settings
    #[serde(rename = "base_node")]
    pub base_node_service_config: BaseNodeServiceConfig,
    /// The relative path to store persistent data
    pub data_dir: PathBuf,
    /// The relative path to the config directory
    pub config_dir: PathBuf,
    /// The main wallet db file
    pub db_file: PathBuf,
    /// The main wallet db sqlite database backend connection pool size for concurrent reads
    pub db_connection_pool_size: usize,
    /// The main wallet password
    #[serde(deserialize_with = "deserialize_safe_password_option")]
    pub password: Option<SafePassword>,
    /// The auto ping interval to use for contacts liveness data
    #[serde(with = "serializers::seconds")]
    pub contacts_auto_ping_interval: Duration,
    /// How long a contact may be not seen before being determined to be offline
    pub contacts_online_ping_window: usize,
    /// When running the console wallet in command mode, how long to wait for sent transactions.
    #[serde(with = "serializers::seconds")]
    pub command_send_wait_timeout: Duration,
    /// When running the console wallet in command mode, at what "stage" to wait for sent transactions.
    pub command_send_wait_stage: TransactionStage,
    /// Notification script file for a notifier service - allows a wallet to execute a script or program when certain
    /// transaction events are received by the console wallet .
    /// (see example at 'applications/minotari_console_wallet/src/notifier/notify_example.sh')
    pub notify_file: Option<PathBuf>,
    /// If true, a GRPC server will bind to the configured address and listen for incoming GRPC requests.
    pub grpc_enabled: bool,
    /// GRPC bind address of the wallet
    pub grpc_address: Option<Multiaddr>,
    /// GRPC authentication mode
    pub grpc_authentication: GrpcAuthentication,
    /// GRPC tls enabled
    pub grpc_tls_enabled: bool,
    /// A custom base node peer that will be used to obtain metadata from
    pub custom_base_node: Option<String>,
    /// A list of base node peers that the wallet should use for service requests and tracking chain state
    pub base_node_service_peers: StringList,
    /// The amount of times wallet recovery will be retried before being abandoned
    pub recovery_retry_limit: usize,
    /// The default uT fee per gram to use for transaction fees
    pub fee_per_gram: u64,
    /// Number of required transaction confirmations used for UI purposes
    pub num_required_confirmations: u64,
    /// Spin up and use a built-in Tor instance. This only works on macos/linux - requires that the wallet was built
    /// with the optional "libtor" feature flag.
    pub use_libtor: bool,
    /// A path to the file that stores the base node identity and secret key
    pub identity_file: Option<PathBuf>,
    /// The type of wallet software, or specific type of hardware
    pub wallet_type: Option<WalletType>,
    /// The cool down period between balance enquiry checks in seconds; requests faster than this will be ignored.
    /// For specialized wallets processing many batch transactions this setting could be increased to 60 s to retain
    /// responsiveness of the wallet with slightly delayed balance updates
    #[serde(with = "serializers::seconds")]
    pub balance_enquiry_cooldown_period: Duration,
}

impl Default for WalletConfig {
    fn default() -> Self {
        let p2p = P2pConfig {
            datastore_path: PathBuf::from("peer_db/wallet"),
            listener_liveness_check_interval: None,
            ..Default::default()
        };
        Self {
            override_from: None,
            p2p,
            transaction_service_config: Default::default(),
            output_manager_service_config: Default::default(),
            buffer_size: 50_000,
            network: Default::default(),
            base_node_service_config: Default::default(),
            data_dir: PathBuf::from_str("data/wallet").unwrap(),
            config_dir: PathBuf::from_str("config/wallet").unwrap(),
            db_file: PathBuf::from_str("db/console_wallet.db").unwrap(),
            db_connection_pool_size: 16, // Note: Do not reduce this default number
            password: None,
            contacts_auto_ping_interval: Duration::from_secs(30),
            contacts_online_ping_window: 30,
            command_send_wait_stage: TransactionStage::Broadcast,
            command_send_wait_timeout: Duration::from_secs(300),
            notify_file: None,
            grpc_enabled: false,
            grpc_address: None,
            grpc_authentication: GrpcAuthentication::default(),
            grpc_tls_enabled: false,
            custom_base_node: None,
            base_node_service_peers: StringList::default(),
            recovery_retry_limit: 3,
            fee_per_gram: 5,
            num_required_confirmations: 3,
            use_libtor: true,
            identity_file: None,
            wallet_type: None,
            balance_enquiry_cooldown_period: Duration::from_secs(5),
        }
    }
}

impl SubConfigPath for WalletConfig {
    fn main_key_prefix() -> &'static str {
        "wallet"
    }
}

impl WalletConfig {
    pub fn set_base_path<P: AsRef<Path>>(&mut self, base_path: P) {
        if !self.data_dir.is_absolute() {
            self.data_dir = base_path.as_ref().join(self.data_dir.as_path());
        }
        if !self.config_dir.is_absolute() {
            self.config_dir = base_path.as_ref().join(self.config_dir.as_path());
        }
        if !self.db_file.is_absolute() {
            self.db_file = self.data_dir.join(self.db_file.as_path());
        }
        self.p2p.set_base_path(base_path);
    }
}

#[derive(Debug, EnumString, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub enum TransactionStage {
    Initiated,
    DirectSendOrSaf,
    Negotiated,
    Broadcast,
    MinedUnconfirmed,
    Mined,
    TimedOut,
}
