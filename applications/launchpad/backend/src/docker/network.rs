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
    if !network_exists(&docker_network).await? {
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