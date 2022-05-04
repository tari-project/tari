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
//

use std::{convert::TryFrom, path::PathBuf, time::Duration};

use derivative::Derivative;
use log::*;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Wry};

use crate::{
    commands::AppState,
    docker::{
        helpers::create_password,
        BaseNodeConfig,
        DockerWrapperError,
        LaunchpadConfig,
        MmProxyConfig,
        Sha3MinerConfig,
        TariNetwork,
        TariWorkspace,
        WalletConfig,
        XmRigConfig,
        DEFAULT_MINING_ADDRESS,
    },
    error::LauncherError,
};

#[derive(Derivative, Serialize, Deserialize)]
#[derivative(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct WorkspaceLaunchOptions {
    root_folder: String,
    tari_network: String,
    has_base_node: bool,
    has_wallet: bool,
    has_sha3_miner: bool,
    has_mm_proxy: bool,
    has_xmrig: bool,
    wait_for_tor: Option<u64>,
    #[derivative(Debug = "ignore")]
    #[serde(skip_serializing)]
    wallet_password: Option<String>,
    sha3_mining_threads: Option<usize>,
    monerod_url: Option<String>,
    monero_username: Option<String>,
    #[derivative(Debug = "ignore")]
    #[serde(skip_serializing)]
    monero_password: Option<String>,
    monero_use_auth: Option<bool>,
    monero_mining_address: Option<String>,
    docker_registry: Option<String>,
    docker_tag: Option<String>,
}

impl TryFrom<WorkspaceLaunchOptions> for LaunchpadConfig {
    type Error = LauncherError;

    fn try_from(options: WorkspaceLaunchOptions) -> Result<Self, Self::Error> {
        let tari_network = TariNetwork::try_from(options.tari_network.to_lowercase().as_str())?;
        let tor_control_password = create_password(16);
        let tor_delay = Duration::from_secs(options.wait_for_tor.unwrap_or(10));
        let base_node = if options.has_base_node {
            Some(BaseNodeConfig { delay: tor_delay })
        } else {
            None
        };
        let wallet = if options.has_wallet {
            if options.wallet_password.is_none() {
                return Err(LauncherError::ConfigVariableRequired(
                    "wallet".to_string(),
                    "wallet_password".to_string(),
                ));
            }
            Some(WalletConfig {
                delay: tor_delay,
                password: options.wallet_password.unwrap(),
            })
        } else {
            None
        };
        let sha3_miner = if options.has_sha3_miner {
            Some(Sha3MinerConfig {
                delay: Duration::from_secs(options.wait_for_tor.unwrap_or(15)),
                num_mining_threads: options.sha3_mining_threads.unwrap_or(1),
            })
        } else {
            None
        };
        let mm_proxy = if options.has_mm_proxy {
            let mut config = MmProxyConfig {
                delay: Duration::from_secs(options.wait_for_tor.unwrap_or(15)),
                ..Default::default()
            };
            if let Some(val) = options.monerod_url {
                config.monerod_url = val;
            }
            if let Some(val) = options.monero_username {
                config.monero_username = val;
            }
            if let Some(val) = options.monero_password {
                config.monero_password = val;
            }
            if let Some(val) = options.monero_use_auth {
                config.monero_use_auth = val;
            }
            Some(config)
        } else {
            None
        };
        let xmrig = if options.has_xmrig {
            let monero_mining_address = options
                .monero_mining_address
                .unwrap_or_else(|| DEFAULT_MINING_ADDRESS.to_string());
            Some(XmRigConfig {
                delay: Duration::from_secs(options.wait_for_tor.unwrap_or(20)),
                monero_mining_address,
            })
        } else {
            None
        };
        Ok(LaunchpadConfig {
            data_directory: PathBuf::from(options.root_folder),
            tari_network,
            tor_control_password,
            base_node,
            wallet,
            sha3_miner,
            mm_proxy,
            xmrig,
            registry: options.docker_registry,
            tag: options.docker_tag,
        })
    }
}

/// This is an example of how we might launch a recipe. It's not currently used in the front-end, but essentially it
/// launches all the containers in a choreographed fashion to stand up the entire mining infra.
#[tauri::command]
pub async fn launch_docker(app: AppHandle<Wry>, name: String, config: WorkspaceLaunchOptions) -> Result<(), String> {
    launch_docker_impl(app, name, config)
        .await
        .map_err(|e| e.chained_message())
}

async fn launch_docker_impl(
    app: AppHandle<Wry>,
    name: String,
    config: WorkspaceLaunchOptions,
) -> Result<(), LauncherError> {
    debug!("Starting docker launch sequence");
    let state = app.state::<AppState>();
    let docker = state.docker_handle().await;
    let should_create_workspace = {
        let wrapper = state.workspaces.read().await;
        !wrapper.workspace_exists(name.as_str())
    }; // drop read-only lock
    {
        let mut wrapper = state.workspaces.write().await;
        // Now that we have a write lock, we can create a new workspace if required
        if should_create_workspace {
            let config = LaunchpadConfig::try_from(config)?;
            info!(
                "Docker workspace does not exist. Creating one with config, {:#?}",
                config
            );
            wrapper.create_workspace(name.as_str(), config)?;
        }
        info!("Launching Tari workspace, {}", name);
        let workspace: &mut TariWorkspace = wrapper
            .get_workspace_mut(name.as_str())
            .ok_or(DockerWrapperError::UnexpectedError)?;
        // Pipe docker container logs to Tauri using namespaced events
        workspace.start_recipe(docker.clone()).await?;
    } // Drop write lock
    info!("Tari system, {} has launched", name);
    Ok(())
}
