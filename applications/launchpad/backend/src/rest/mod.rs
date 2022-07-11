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

pub mod quay_io;
use std::collections::HashMap;

use bollard::{
    image::{CreateImageOptions, ListImagesOptions, SearchImagesOptions},
    models::{CreateImageInfo, ImageSummary},
};
use futures::{Stream, StreamExt, TryStreamExt};
use log::{debug, error};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use self::quay_io::QUAY_IO_REPO_NAME;
use crate::{
    docker::{DockerWrapperError, ImageType, TariWorkspace, DOCKER_INSTANCE},
    rest::quay_io::TARILABS_REPO_NAME,
};

#[derive(Debug, Error)]
pub enum DockerImageError {
    #[error("The image {0} is not found")]
    ImageNotFound(String),
    #[error("Something went wrong with the Docker API")]
    DockerError(#[from] bollard::errors::Error),
    #[error("Could not create an identity file")]
    InvalidImageType,
}

pub async fn list_image(fully_qualified_image_name: String) -> Result<Vec<ImageSummary>, DockerImageError> {
    let docker = DOCKER_INSTANCE.clone();
    let mut fillter: HashMap<String, Vec<String>> = HashMap::new();
    fillter.insert("reference".to_string(), vec![fully_qualified_image_name.clone()]);
    let result = docker
        .list_images(Some(ListImagesOptions::<String> {
            all: true,
            filters: fillter,
            ..Default::default()
        }))
        .await
        .map_err(|err| {
            error!("Error searching for{}. Err: {}", fully_qualified_image_name, err);
            DockerImageError::ImageNotFound(fully_qualified_image_name)
        })?;
    Ok(result)
}

#[tokio::test]
#[ignore]
async fn find_image_test() {
    let result = list_image("quay.io/tarilabs/tari_base_node:latest-amd64".to_string()).await;
    println!("result {:?}", result);
}
