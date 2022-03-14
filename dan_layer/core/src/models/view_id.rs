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

use std::{
    cmp::Ordering,
    fmt::{self, Display},
    ops::{Add, Sub},
};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct ViewId(pub u64);

impl ViewId {
    pub fn current_leader(&self, committee_size: usize) -> usize {
        (self.0 % committee_size as u64) as usize
    }

    pub fn is_genesis(&self) -> bool {
        self.0 == 0
    }

    pub fn next(&self) -> ViewId {
        ViewId(self.0 + 1)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }

    pub fn saturating_sub(self, other: ViewId) -> ViewId {
        self.0.saturating_sub(other.0).into()
    }
}

impl PartialOrd for ViewId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl From<u64> for ViewId {
    fn from(v: u64) -> Self {
        Self(v)
    }
}

impl Display for ViewId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "View({})", self.0)
    }
}

impl Add for ViewId {
    type Output = ViewId;

    fn add(self, rhs: Self) -> Self::Output {
        ViewId(self.0 + rhs.0)
    }
}

impl Sub for ViewId {
    type Output = ViewId;

    fn sub(self, rhs: Self) -> Self::Output {
        ViewId(self.0 - rhs.0)
    }
}
