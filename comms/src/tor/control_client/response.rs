// Copyright 2020, The Tari Project
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

use std::borrow::Cow;

const OK_CODE: u16 = 250;
pub const EVENT_CODE: u16 = 650;

/// Represents a single response line from the server.
#[derive(Debug)]
pub struct ResponseLine<'a> {
    pub(super) value: Cow<'a, str>,
    pub(super) code: u16,
    pub(super) has_more: bool,
    pub(super) is_multiline: bool,
}

impl<'a> ResponseLine<'a> {
    pub fn is_ok(&self) -> bool {
        self.code == OK_CODE
    }

    pub fn has_more(&self) -> bool {
        self.has_more
    }

    pub fn into_owned<'b>(self) -> ResponseLine<'b> {
        ResponseLine {
            value: Cow::Owned(self.value.into_owned()),
            code: self.code,
            has_more: self.has_more,
            is_multiline: self.is_multiline,
        }
    }

    pub fn is_err(&self) -> bool {
        !self.is_ok()
    }

    pub fn err(&self) -> Option<Cow<'a, str>> {
        if self.is_err() {
            Some(self.value.clone())
        } else {
            None
        }
    }
}
