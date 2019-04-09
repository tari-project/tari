// Copyright 2019 The Tari Project
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

#![recursion_limit = "128"]

extern crate proc_macro;
extern crate proc_macro2;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

mod extend_bytes;
mod hashable;
mod hashable_ordering;

/// This macro will produce the 4 trait implementations required for an hashable struct to be sorted
#[proc_macro_derive(HashableOrdering)]
pub fn derive_hashable_ordering(tokens: TokenStream) -> TokenStream {
    hashable_ordering::create_hashable_ordering(tokens)
}

/// This macro will provide a Hashable implementation to the a given struct using a Digest implementing Hash function
/// To use this provide #[derive(Hashable)] to the struct and #[Digest = "<Digest>"] with <Digest> being the included
/// digest the macro should use to impl Hashable individual fields can be skipped by providing them with:
/// #[Hashable(Ignore)]
#[proc_macro_derive(Hashable, attributes(digest, Hashable, ExtendBytes))]
pub fn derive_hashable(tokens: TokenStream) -> TokenStream {
    let input = parse_macro_input!(tokens as DeriveInput);
    let hash = hashable::create_derive_hashable(input.clone());
    let extendbytes = extend_bytes::create_derive_extend_bytes(input);
    let tokens = quote! {
        #hash
        #extendbytes
    };
    tokens.into()
}

/// This macro will provide a To_bytes implementation to the a given struct
/// To use this provide #[derive(ExtendBytes)] to the struct
/// digest the macro should use to impl Hashable individual fields can be skipped by providing them with:
/// #[ExtendBytes(Ignore)]
#[proc_macro_derive(ExtendBytes, attributes(ExtendBytes))]
pub fn derive_to_bytes(tokens: TokenStream) -> TokenStream {
    let input = parse_macro_input!(tokens as DeriveInput);
    extend_bytes::create_derive_extend_bytes(input).into()
}
