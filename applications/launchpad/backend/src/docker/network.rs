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

use bollard::{network::{ConnectNetworkOptions, InspectNetworkOptions, CreateNetworkOptions}, models::{EndpointSettings, Network}};
use log::{info, warn};

use crate::docker::DOCKER_INSTANCE;

use super::{DockerWrapperError, ContainerId, ImageType};

pub async fn network_exists(network_name: &str) -> Result<bool, DockerWrapperError> {
    let options = InspectNetworkOptions {
        verbose: false,
        scope: "local",
    };
    let network = &DOCKER_INSTANCE.inspect_network(network_name, Some(options)).await;
    // hardcore pattern matching yo!
    if let Ok(Network {
        name: Some(network_name),
        id: Some(id),
        ..
    }) = network
    {
        info!("Network {} (id:{}) exists", network_name, id);
        Ok(true)
    } else {
        info!("Network {} does not exist", network_name);
        Ok(false)
    }
}

/// Create a network in docker to allow the containers in this workspace to communicate with each other.
pub async fn create_network(network: &str) -> Result<(), DockerWrapperError> {
    let options = CreateNetworkOptions {
        name: network,
        check_duplicate: true,
        driver: "bridge",
        internal: false,
        attachable: false,
        ingress: false,
        ipam: Default::default(),
        enable_ipv6: false,
        options: Default::default(),
        labels: Default::default(),
    };
    let res = &DOCKER_INSTANCE.create_network(options).await?;
    if let Some(id) = (*res).clone().id {
        info!("Network {} (id:{}) created", network, id);
    }
    if let Some(warn) = (*res).clone().warning {
        warn!("Creating {} network had warnings: {}", network, warn);
    }
    Ok(())
}

//docker network name following the pattern: {tari_network}_netwrok
pub fn network_name(network: &str) -> String {
    format!("{}_network", network)
}


//Creates new docker network with name {tari_network}_network if not exist.
pub async fn try_create_network(tari_network: &str) -> Result<(), DockerWrapperError> {
    let docker_network = network_name(tari_network);
    info!("trying to connect to network: {}", docker_network);
    // Check network requirements for the service
    if let Ok(false) = network_exists(&docker_network).await {
        create_network(&docker_network).await?;
        Ok(())
    } else {
        info!("network {} already exists", docker_network);
        Ok(())
    }
}

/// Connects a container to the workspace network. This is not typically needed, since the container will
/// automatically be connected to the network when it is created in [`start_service`].
pub async fn connect_to_network(network: &str, id: &ContainerId, image: ImageType) -> Result<(), DockerWrapperError> {
    let docker_network = format!("{}_network", network);
    let options = ConnectNetworkOptions {
        container: id.as_str(),
        endpoint_config: EndpointSettings {
            aliases: Some(vec![image.container_name().to_string()]),
            ..Default::default()
        },
    };
    info!(
        "Connecting container {} ({}) to network {}...",
        image.image_name(),
        id,
        network
    );
    let _ = &DOCKER_INSTANCE
        .connect_network(docker_network.as_str(), options)
        .await?;
    info!(
        "Docker container {} ({}) connected to network {}",
        image.image_name(),
        id,
        network
    );
    Ok(())
}