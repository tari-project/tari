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

use std::collections::HashMap;

use bollard::{
    container::{InspectContainerOptions, ListContainersOptions, StatsOptions, TopOptions},
    models::{ContainerStateStatusEnum, ContainerSummary, ContainerTopResponse},
    Docker,
};
use futures::{
    stream::{self, FuturesUnordered},
    StreamExt,
    TryFutureExt,
};

use crate::docker::{
    container_state,
    filter,
    ContainerStatus,
    DockerWrapperError,
    ImageType,
    CONTAINERS,
    DOCKER_INSTANCE,
};

pub const NOT_FOUND: &str = "not found";

/// Read current container's status from Docker.
/// If container is not found then not.found is returned.
pub async fn container_status(container_id_or_name: &str) -> String {
    let container_status = docker_info(&DOCKER_INSTANCE, container_id_or_name).await;
    match container_status {
        Ok(status) => status,
        Err(_) => NOT_FOUND.to_string(),
    }
}

/// Returns the status for the image. If container is not created not found is
pub async fn status(image: ImageType) -> String {
    let current_state = match container_state(image.container_name()) {
        Some(state) => state,
        None => return NOT_FOUND.to_string(),
    };
    container_status(current_state.id().as_str()).await
}

async fn docker_info(docker: &Docker, container_id: &str) -> Result<String, DockerWrapperError> {
    let status = docker
        .inspect_container(container_id, None)
        .await?
        .state
        .unwrap()
        .status
        .unwrap()
        .to_string();
    Ok(status)
}

// TODO:  We should add a "requires-docker" flag,
// #[tokio::test]

async fn test_health_check() {
    let status = container_status("ca53c4a003cd").await;
    assert_eq!("running".to_string(), status);

    let status = container_status("xxx_unknown").await;
    assert_eq!(NOT_FOUND.to_string(), status);

    let status = container_status("default_tor").await;
    assert_eq!("running".to_string(), status);
}
