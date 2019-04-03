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
use quote::{quote, quote_spanned};
use syn::{spanned::Spanned, Data, DeriveInput, Fields, Index};

// this is the actual code for the derive macro, the function in lib points to this one
pub fn create_derive_extend_bytes(input: DeriveInput) -> proc_macro2::TokenStream {
    let object_name = &input.ident;
    let item = input.data;
    let fields_text = handle_fields_for_extend_bytes(&item);
    let gen = quote! {
        impl  ExtendBytes for #object_name  {
            fn append_raw_bytes(&self, buf: &mut Vec<u8>) {
                #fields_text
            }
        }
    };
    gen
}

// this function processes the individual fields of the hashable trait macro: derive_hashable
fn handle_fields_for_extend_bytes(item: &Data) -> proc_macro2::TokenStream {
    match item {
        Data::Struct(ref item) => {
            match item.fields {
                Fields::Named(ref fields) => {
                    let recurse = fields.named.iter().map(|f| {
                        let mut do_we_ignore_field = false;
                        for attr in &f.attrs {
                            match attr.interpret_meta().unwrap() {
                                syn::Meta::NameValue(ref val) => {
                                    if val.ident.to_string() == "ExtendBytes" {
                                        if let syn::Lit::Str(lit) = &val.lit {
                                            if lit.value() == "Ignore" {
                                                do_we_ignore_field = true;
                                            }
                                        }
                                    }
                                },
                                syn::Meta::List(ref val) => {
                                    // we have more than one property
                                    if val.ident.to_string() == "ExtendBytes" {
                                        // we have a hash command here, lets search for the sub command
                                        for nestedmeta in val.nested.iter() {
                                            if let syn::NestedMeta::Meta(meta) = nestedmeta {
                                                if let syn::Meta::Word(ref val) = meta {
                                                    if val.to_string() == "Ignore" {
                                                        do_we_ignore_field = true;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                },
                                _ => (),
                            };
                        }
                        if !do_we_ignore_field {
                            let name = &f.ident;
                            quote_spanned! {f.span()=>
                            (&self.#name).append_raw_bytes(buf);
                            }
                        } else {
                            quote_spanned! {f.span()=>
                            }
                        }
                    });
                    quote! {#( #recurse)*
                    }
                },
                Fields::Unnamed(ref fields) => {
                    let recurse = fields.unnamed.iter().enumerate().map(|(i, f)| {
                        let index = Index::from(i);
                        quote_spanned! {f.span()=>
                            (&self.#index).append_raw_bytes(buf);
                        }
                    });
                    quote! {
                         #( #recurse)*
                    }
                },
                Fields::Unit => {
                    // dont hash units
                    quote!(0)
                },
            }
        },
        // have not yet implemented enums and unions
        Data::Enum(_) | Data::Union(_) => unimplemented!(),
    }
}
