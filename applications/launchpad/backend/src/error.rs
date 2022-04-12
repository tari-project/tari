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

use std::error::Error;

use tauri::{api::Error as TauriApiError, Error as TauriError};
use thiserror::Error;

use crate::docker::DockerWrapperError;

#[derive(Debug, Error)]
pub enum LauncherError {
    #[error("Something went wrong with the Docker Wrapper")]
    DockerWrapperError(#[from] DockerWrapperError),
    #[error("Something went wrong on your filesystem")]
    FileSystemError(#[from] std::io::Error),
    #[error("Something went screwy with Tauri")]
    TauriError(#[from] TauriError),
    #[error("Something went awry with the Tauri API")]
    TauriApiError(#[from] TauriApiError),
    #[error("A workspace configuration object is required")]
    MissingConfig,
    #[error("{1} is required because we are creating a {0}")]
    ConfigVariableRequired(String, String),
}

impl LauncherError {
    /// Combine all error messages down the chain into one string.
    pub fn chained_message(&self) -> String {
        let mut messages = vec![self.to_string()];
        let mut this = self as &dyn Error;
        while let Some(next) = this.source() {
            messages.push(next.to_string());
            this = next;
        }
        messages.join(" caused by:\n")
    }
}
