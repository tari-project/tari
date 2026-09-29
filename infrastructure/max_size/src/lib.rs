//  Copyright 2022. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

//! Size bounded wrappers: [`MaxSizeBytes`], [`MaxSizeString`] and [`MaxSizeVec`].
//!
//! Each type guarantees that its length is at most its `MAX` parameter, and enforces this in every constructor and
//! in every decoder (borsh and serde), so the bound holds for values decoded from untrusted input.
//!
//! The bound is a *length* bound and nothing else. In particular [`MaxSizeString`] only guarantees valid UTF-8 of at
//! most `MAX` **bytes**: there is no character set restriction and no validation of what the string is supposed to
//! be (a URL, a name, ...). Its `Display` implementation writes the raw string, so callers must escape it (`{:?}` or
//! `str::escape_debug`) before writing an untrusted value to a log or a terminal.

mod bounded_serde;
mod checked_de;
mod string;
pub use string::{MaxSizeString, MaxSizeStringLengthError};
mod bytes;
pub use bytes::{MaxSizeBytes, MaxSizeBytesError};
mod vec;
pub use vec::{MaxSizeVec, MaxSizeVecError};
