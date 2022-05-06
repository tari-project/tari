use bollard::models::ContainerCreateResponse;
use log::debug;

use super::{ContainerId, ContainerState, ContainerStatus, DockerWrapperError, CONTAINERS};

/// Add the container info to the list of containers the wrapper is managing
pub fn add_container(id: &str, state: ContainerState) {
    // let id = ContainerId::from(container.id.clone());
    // let state = ContainerState::new(name.to_string(), id, container);
    CONTAINERS.write().unwrap().insert(id.to_string(), state);
}

// Tag the container with name/id `id` as Running
pub fn change_container_status(id: &str, status: ContainerStatus) -> Result<(), DockerWrapperError> {
    if let Some(container) = CONTAINERS.write().unwrap().get_mut(id) {
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
        Err(DockerWrapperError::ContainerNotFound(id.to_string()))
    }
}
///Get the state of the container by name or id.
pub fn container_state(id: &str) -> Result<ContainerState, DockerWrapperError> {
    if let Some(container) = CONTAINERS.read().unwrap().get(id) {
        Ok((*container).clone())
    } else {
        Err(DockerWrapperError::ContainerNotFound(id.to_string()))
    }
}
///Remove the container and state.
pub fn remove_container(id: &str) -> Result<(), DockerWrapperError> {
    if let Some(_state) = CONTAINERS.write().unwrap().remove(id) {
        Ok(())
    } else {
        Err(DockerWrapperError::ContainerNotFound(id.to_string()))
    }
}

#[test]
fn create_get_update_and_delete_container_test() {
    let state = ContainerState::new(
        "tor".to_string(),
        ContainerId("1234".to_string()),
        ContainerCreateResponse {
            id: "1234".to_string(),
            warnings: vec![],
        },
    );
    add_container("tor", state.clone());
    assert_eq!(1, CONTAINERS.read().unwrap().len());

    let value = container_state("tor").unwrap();
    assert_eq!(state.name(), value.clone().name());
    assert_eq!(state.id(), value.id());
    assert_eq!(ContainerStatus::Created, value.status());

    change_container_status("tor", ContainerStatus::Running).unwrap();
    let value = container_state("tor").unwrap();
    assert_eq!(ContainerStatus::Running, value.status());

    change_container_status("tor", ContainerStatus::Deleted).unwrap();
    let value = container_state("tor").unwrap();
    assert_eq!(ContainerStatus::Deleted, value.status());

    remove_container("tor").unwrap();
    assert!(container_state("tor").is_err());
    assert!(change_container_status("tor", ContainerStatus::Running).is_err());
    assert!(remove_container("tor").is_err());
}
