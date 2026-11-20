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

use proc_macro::TokenStream;
use quote::quote;

// this is the actual code for the hashable ordering macro, function in lib points to this one
pub fn create_hashable_ordering(tokens: TokenStream) -> TokenStream {
    // Parse TokenStream into AST
    let ast: syn::DeriveInput = syn::parse(tokens).unwrap();
    create_derive_hashable_ordering(ast).into()
}

pub fn create_derive_hashable_ordering(ast: syn::DeriveInput) -> proc_macro2::TokenStream {
    let name = &ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();
    quote! {
        impl #impl_generics ::std::cmp::Ord for #name #ty_generics #where_clause {
            fn cmp(&self, other: &#name #ty_generics) -> ::std::cmp::Ordering {
                self.hash().cmp(&other.hash())
            }
        }
        impl #impl_generics ::std::cmp::PartialOrd for #name #ty_generics #where_clause {
            fn partial_cmp(&self, other: &#name #ty_generics) -> ::std::option::Option<::std::cmp::Ordering> {
                ::std::option::Option::Some(self.cmp(other))
            }
        }
        impl #impl_generics ::std::cmp::PartialEq for #name #ty_generics #where_clause {
            fn eq(&self, other: &#name #ty_generics) -> bool {
                self.hash() == other.hash()
            }
        }
        impl #impl_generics ::std::cmp::Eq for #name #ty_generics #where_clause {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashable_ordering_uses_fully_qualified_ordering_and_generics() {
        let input: syn::DeriveInput = syn::parse_str("struct Item<T>(T);").unwrap();
        let tokens = create_derive_hashable_ordering(input).to_string();
        assert!(tokens.contains(":: std :: cmp :: Ordering"));
        assert!(tokens.contains("Item < T >"));
    }
}
