// Copyright 2023. The Taiji Project
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

use std::fmt::{Display, Formatter};

use super::Status;

pub struct Feature {
    name: &'static str,
    description: &'static str,
    tracking_issue: Option<usize>,
    status: Status,
}

impl Feature {
    pub const fn new(
        name: &'static str,
        description: &'static str,
        tracking_issue: Option<usize>,
        status: Status,
    ) -> Self {
        Feature {
            name,
            description,
            tracking_issue,
            status,
        }
    }

    pub fn issue_url(&self) -> String {
        match self.tracking_issue {
            Some(n) => format!("https://github.com/taiji-project/taiji/issues/{}", n),
            None => "None".into(),
        }
    }

    pub fn attr_name(&self) -> String {
        format!("taiji_feature_{}", self.name)
    }

    pub fn is_active_in_testnet(&self) -> bool {
        matches!(self.status, Status::New | Status::Testing)
    }

    pub fn is_active_in_nextnet(&self) -> bool {
        matches!(self.status, Status::Testing)
    }

    pub fn is_active(&self) -> bool {
        matches!(self.status, Status::Active)
    }

    pub fn was_removed(&self) -> bool {
        matches!(self.status, Status::Removed)
    }
}

impl Display for Feature {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}. {}. Tracking issue: {}",
            self.name,
            self.description,
            self.issue_url()
        )
    }
}
