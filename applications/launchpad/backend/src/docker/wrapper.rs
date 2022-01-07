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
    image::CreateImageOptions,
    models::{CreateImageInfo, SystemEventsResponse},
    system::EventsOptions,
    Docker,
};
use futures::{Stream, TryStreamExt};

use crate::docker::DockerWrapperError;

/// A wrapper around a [`bollard::Docker`] instance providing some opinionated convenience methods for Tari workspaces.
pub struct DockerWrapper {
    handle: Docker,
}

impl DockerWrapper {
    /// Create a new wrapper
    pub fn new() -> Result<Self, DockerWrapperError> {
        let handle = Docker::connect_with_local_defaults()?;
        Ok(Self { handle })
    }

    /// Returns the version of the _docker client_.
    pub fn version(&self) -> String {
        self.handle.client_version().to_string()
    }

    /// Create a (cheap) clone of the [`Docker`] instance, suitable for passing to threads and futures.
    pub fn handle(&self) -> Docker {
        self.handle.clone()
    }

    /// Pull an image from a repository.
    ///
    /// image_name: The fully qualified name of the image, {registry}/{name}:{tag}
    /// To use the default registry and tag, you can call `Self::fully_qualified_image(image, registry, tag)`
    /// to build a default full-qualified image name.
    pub async fn pull_image(
        &self,
        image_name: String,
    ) -> impl Stream<Item = Result<CreateImageInfo, DockerWrapperError>> {
        let opts = Some(CreateImageOptions {
            from_image: image_name,
            ..Default::default()
        });
        let stream = self.handle.create_image(opts, None, None);
        stream.map_err(DockerWrapperError::from)
    }

    /// Returns a stream of relevant events. We're opinionated here, so we filter the stream to only return
    /// container, image, network and volume events.
    pub async fn events(&self) -> impl Stream<Item = Result<SystemEventsResponse, DockerWrapperError>> {
        let docker = self.handle.clone();
        let mut type_filter = HashMap::new();
        type_filter.insert("type".to_string(), vec![
            "container".to_string(),
            "image".to_string(),
            "network".to_string(),
            "volume".to_string(),
        ]);
        let options = EventsOptions {
            since: None,
            until: None,
            filters: type_filter,
        };
        docker.events(Some(options)).map_err(DockerWrapperError::from)
    }
}
