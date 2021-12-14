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

use crate::{
    commands::AppState,
    docker::{
        helpers::create_password,
        BaseNodeConfig,
        ContainerId,
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
use bollard::Docker;
use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};
use std::{convert::TryFrom, path::PathBuf, time::Duration};
use tauri::{AppHandle, Manager, Wry};

use log::*;

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceLaunchOptions {
    root_folder: String,
    tari_network: String,
    has_base_node: bool,
    has_wallet: bool,
    has_sha3_miner: bool,
    has_mm_proxy: bool,
    has_xmrig: bool,
    wait_for_tor: Option<u64>,
    wallet_password: Option<String>,
    sha3_mining_threads: Option<usize>,
    monerod_url: Option<String>,
    monero_username: Option<String>,
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
        let base_node = match options.has_base_node {
            false => None,
            true => Some(BaseNodeConfig {
                delay: tor_delay,
            }),
        };
        let wallet = match options.has_wallet {
            false => None,
            true => {
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
            },
        };
        let sha3_miner = match options.has_sha3_miner {
            false => None,
            true => Some(Sha3MinerConfig {
                delay: Duration::from_secs(options.wait_for_tor.unwrap_or(15)),
                num_mining_threads: options.sha3_mining_threads.unwrap_or(1),
            }),
        };
        let mm_proxy = match options.has_mm_proxy {
            false => None,
            true => {
                let mut config = MmProxyConfig {
                    delay: Duration::from_secs(options.wait_for_tor.unwrap_or(15)),
                    .. Default::default()
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
            },
        };
        let xmrig = match options.has_xmrig {
            false => None,
            true => {
                let monero_mining_address = options
                    .monero_mining_address
                    .unwrap_or_else(|| DEFAULT_MINING_ADDRESS.to_string());
                Some(XmRigConfig {
                    delay: Duration::from_secs(options.wait_for_tor.unwrap_or(20)),
                    monero_mining_address
                })
            },
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
        workspace.start(docker.clone()).await?;
        pipe_container_logs(app.clone(), docker.clone(), workspace);
    } // Drop write lock
    info!("Tari system, {} has launched", name);
    Ok(())
}

pub fn log_event_name(workspace: &mut TariWorkspace, id: &ContainerId) -> String {
    let name = workspace
        .managed_containers()
        .get(id)
        .map(|state| state.name())
        .unwrap_or("unknown");
    format!("tari://docker_log_{}", name)
}

fn pipe_container_logs(app: AppHandle<Wry>, docker: Docker, workspace: &mut TariWorkspace) {
    info!("Setting up log events");
    let streams = workspace.logs(docker);
    for (id, mut stream) in streams {
        let event_name = log_event_name(workspace, &id);
        let app_clone = app.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(message) = stream.next().await {
                let emit_result = match message {
                    Ok(payload) => app_clone.emit_all(event_name.as_str(), payload),
                    Err(err) => app_clone.emit_all(format!("{}_error", event_name).as_str(), err.chained_message()),
                };
                if let Err(err) = emit_result {
                    warn!("Error emitting event: {}", err.to_string());
                }
            }
            info!("Log stream for {} has closed.", id);
        });
    }
    info!("Container log events configured.");
}
