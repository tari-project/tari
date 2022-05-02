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

use tari_common::exit_codes::ExitError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DockerWrapperError {
    #[error("Something went wrong with the Docker API")]
    DockerError(#[from] bollard::errors::Error),
    #[error("Something went wrong on your filesystem")]
    FileSystemError(#[from] std::io::Error),
    #[error("The requested container id, {0} is not being managed by the wrapper")]
    ContainerNotFound(String),
    #[error("The designated workspace, {0}, already exists")]
    WorkspaceAlreadyExists(String),
    #[error("The designated workspace, {0}, does not exist")]
    WorkspaceDoesNotExist(String),
    #[error("The network is not supported")]
    UnsupportedNetwork,
    #[error("It should not be possible to be in this error state")]
    UnexpectedError,
    #[error("Could not create an identity file")]
    IdentityError(#[from] ExitError),
    #[error("The specified image type is not supported")]
    InvalidImageType,
}

impl DockerWrapperError {
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
