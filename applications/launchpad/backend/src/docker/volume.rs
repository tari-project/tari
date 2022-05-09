use bollard::volume::CreateVolumeOptions;
use log::{trace, info};

use crate::docker::DOCKER_INSTANCE;

use super::DockerWrapperError;



/// Checks whether the blockchain data volume exists
pub async fn volume_exists(workspace_name: &str, network: &str) -> Result<bool, DockerWrapperError> {
    let name = tari_blockchain_volume_name(workspace_name, network);
    let volume = &DOCKER_INSTANCE.inspect_volume(name.as_str()).await?;
    trace!("Volume {} exists at {}", name, volume.mountpoint);
    Ok(true)
}

/// Creates docker volume name following the pattern: {workspace}_{tari_network}_{image_name}
pub fn tari_blockchain_volume_name(workspace_name: &str, network: &str) -> String {
    format!("{}_{}_volume", workspace_name, network)
}


/// Tries to create a new blockchain data volume for this workspace.
pub async fn create_volume(workspace_name: &str, network: &str) -> Result<(), DockerWrapperError> {
    let name = tari_blockchain_volume_name(workspace_name, network);
    let config = CreateVolumeOptions {
        name,
        driver: "local".to_string(),
        ..Default::default()
    };
    let volume = &DOCKER_INSTANCE.create_volume(config).await?;
    info!("Docker volume {} created at {}", volume.name, volume.mountpoint);
    Ok(())
}