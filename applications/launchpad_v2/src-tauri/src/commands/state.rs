use bollard::Docker;
use tauri::PackageInfo;
use tokio::sync::RwLock as AsyncLock;

// Copyright 2021. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the
// following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED
// WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
// PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY
// DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO,
// PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
// CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR
// OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH
// DAMAGE.
use crate::docker::{DockerWrapper, Workspaces};

/// The state variables that are managed by Tauri. Since the whole app is asynchronous, we need to store
/// * the [`Workspaces`] object. This tracks global config settings, which containers we are running, and what their
///   status is.
/// * A handle to the docker API, via the [`DockerWrapper`] struct. The convenience function [`docker_handle`] gives you
///   a cheap, thread-safe clone to the underlying docker API.
/// * Package info, because Tauri doesn't give us an easy way to get at that in command callbacks for some reason.
///
/// Things that are mutable (docker, workspaces) are behind an [`AsyncLock`] for thread safety.
pub struct AppState {
    pub docker: AsyncLock<DockerWrapper>,
    pub workspaces: AsyncLock<Workspaces>,
    pub package_info: PackageInfo,
}

impl AppState {
    pub fn new(docker: DockerWrapper, workspaces: Workspaces, package_info: PackageInfo) -> Self {
        Self {
            docker: AsyncLock::new(docker),
            workspaces: AsyncLock::new(workspaces),
            package_info,
        }
    }

    pub async fn docker_handle(&self) -> Docker {
        let wrapper = self.docker.read().await;
        wrapper.handle()
    }
}
