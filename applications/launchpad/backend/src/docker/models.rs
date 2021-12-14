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

use bollard::{container::LogOutput, models::ContainerCreateResponse};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

//-------------------------------------------     ContainerId      ----------------------------------------------
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContainerId(String);

impl From<String> for ContainerId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl AsRef<str> for ContainerId {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl Display for ContainerId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.as_str())
    }
}

impl ContainerId {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

//-------------------------------------------     ContainerStatus      ----------------------------------------------

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ContainerStatus {
    Created,
    Running,
    Stopped,
    Deleted,
}

//-------------------------------------------     ContainerState      ----------------------------------------------

pub struct ContainerState {
    name: String,
    info: ContainerCreateResponse,
    status: ContainerStatus,
}

impl ContainerState {
    pub fn new(name: String, info: ContainerCreateResponse) -> Self {
        Self {
            name,
            info,
            status: ContainerStatus::Created,
        }
    }

    pub fn running(&mut self) {
        self.status = ContainerStatus::Running;
    }

    pub fn set_stop(&mut self) {
        self.status = ContainerStatus::Stopped;
    }

    pub fn set_deleted(&mut self) {
        self.status = ContainerStatus::Deleted;
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn info(&self) -> &ContainerCreateResponse {
        &self.info
    }

    pub fn status(&self) -> ContainerStatus {
        self.status
    }
}

//-------------------------------------------     LogMessage      ----------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogMessage {
    pub message: String,
    pub source: String,
}

impl From<LogOutput> for LogMessage {
    fn from(log: LogOutput) -> Self {
        let (source, message) = match log {
            LogOutput::StdErr { message } => ("StdErr".to_string(), String::from_utf8_lossy(&message).into_owned()),
            LogOutput::StdOut { message } => ("StdOut".to_string(), String::from_utf8_lossy(&message).into_owned()),
            LogOutput::Console { message } => ("Console".to_string(), String::from_utf8_lossy(&message).into_owned()),
            LogOutput::StdIn { message } => ("StdIn".to_string(), String::from_utf8_lossy(&message).into_owned()),
        };
        Self { source, message }
    }
}
