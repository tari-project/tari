//  Copyright 2021, The Tari Project
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

//! # Covenants
//!
//! Allows rules to be specified that restrict _future_ spending of subsequent transactions.
//!
//! <https://rfc.tari.com/RFC-0250_Covenants.html>

mod arguments;
mod byte_codes;
mod context;
mod covenant;
mod decoder;
mod encoder;
mod error;
mod fields;
mod filters;
mod output_set;
mod serde;
mod token;

pub use covenant::Covenant;
pub use error::CovenantError;
// Used in macro
#[allow(unused_imports)]
pub(crate) use fields::OutputField;
pub use token::CovenantToken;

#[macro_use]
mod macros;

#[cfg(test)]
mod test;

use tari_crypto::hash_domain;

hash_domain!(BaseLayerCovenantsDomain, "com.tari.base_layer.covenants", 1);

pub(crate) const COVENANTS_FIELD_HASHER_LABEL: &str = "fields";
