use bollard::models::ContainerCreateResponse;
use log::debug;

use super::{ContainerId, CONTAINERS, ContainerState, DockerWrapperError, ContainerStatus};

 /// Add the container info to the list of containers the wrapper is managing
 pub fn add_container(name: &str, state: ContainerState) {
    // let id = ContainerId::from(container.id.clone());
    // let state = ContainerState::new(name.to_string(), id, container);
    CONTAINERS.write().unwrap().insert(name.to_string(), state);
}

// Tag the container with id `id` as Running
pub fn change_container_status(name: &str, status: ContainerStatus) -> Result<(), DockerWrapperError> {
    if let Some(container) = CONTAINERS.write().unwrap().get_mut(name) {
        if status == container.status() {
            debug!("Status is already set to: {:?}", status.clone());
        } else {
            match status {
                ContainerStatus::Created => (),
                ContainerStatus::Running => container.running(),
                ContainerStatus::Stopped => container.set_stop(),
                ContainerStatus::Deleted => container.set_deleted(),
            }
        }
        Ok(())
    } else {
        Err(DockerWrapperError::ContainerNotFound(name.to_string()))
    }
}

pub fn container_state(name: &str) -> Result<ContainerState, DockerWrapperError> {
    if let Some(container) = CONTAINERS.read().unwrap().get(name) {
        Ok((*container).clone())
    } else {
        Err(DockerWrapperError::ContainerNotFound(name.to_string()))
    }
}


