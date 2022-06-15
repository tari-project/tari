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

use bollard::models::ContainerCreateResponse;
use log::debug;

use super::{ContainerId, ContainerState, ContainerStatus, DockerWrapperError, CONTAINERS};
use crate::error::LauncherError;

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
/// Get the state of the container by name or id.
pub fn container_state(id: &str) -> Option<ContainerState> {
    CONTAINERS.read().unwrap().get(id).cloned()
}
/// Remove the container and state.
pub fn remove_container(id: &str) -> Result<ContainerState, DockerWrapperError> {
    if let Some(state) = CONTAINERS.write().unwrap().remove(id) {
        Ok(state)
    } else {
        Err(DockerWrapperError::ContainerNotFound(id.to_string()))
    }
}

pub fn filter(status: ContainerStatus) -> Result<Vec<ContainerState>, DockerWrapperError> {
    let snapshot = CONTAINERS.read().unwrap().clone();

    if snapshot.is_empty() {
        return Err(DockerWrapperError::ContainerNotFound(
            r#"No container found"#.to_string(),
        ));
    }
    let found: Vec<ContainerState> = snapshot
        .iter()
        .filter(|(_k, state)| status == state.status())
        .map(|(_k, v)| v)
        .cloned()
        .collect();
    if found.is_empty() {
        debug!("No countainer found with status: {:?}", status);
        return Err(DockerWrapperError::ContainerNotFound(
            r#"No container found"#.to_string(),
        ));
    }
    Ok(found)
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
    assert_eq!(state.name(), value.name());
    assert_eq!(state.id(), value.id());
    assert_eq!(ContainerStatus::Created, value.status());

    change_container_status("tor", ContainerStatus::Running).unwrap();
    let value = container_state("tor").unwrap();
    assert_eq!(ContainerStatus::Running, value.status());

    change_container_status("tor", ContainerStatus::Deleted).unwrap();
    let value = container_state("tor").unwrap();
    assert_eq!(ContainerStatus::Deleted, value.status());

    let deleted = remove_container("tor").unwrap();
    assert_eq!(state.name(), deleted.name());
    assert!(container_state("tor").is_none());
    assert!(change_container_status("tor", ContainerStatus::Running).is_err());
    assert!(remove_container("tor").is_err());
}
