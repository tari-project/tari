#![recursion_limit = "128"]
//#![feature(Hashable_ignore)]

extern crate proc_macro;
extern crate proc_macro2;

use proc_macro::TokenStream;
use proc_macro2::{Ident, Span};
use quote::{quote, quote_spanned};
use syn::{parse_macro_input, spanned::Spanned, Data, DeriveInput, Fields, Index};

/// This macro will produce the 4 trait implementations required for an hashable struct to be sorted
#[proc_macro_derive(HashableOrdering)]
pub fn derive_hashable_ordering(tokens: TokenStream) -> TokenStream {
    // Parse TokenStream into AST
    let ast: syn::DeriveInput = syn::parse(tokens).unwrap();
    let name = &ast.ident;
    let gen = quote! {
         impl Ord for #name {
            fn cmp(&self, other: &#name) -> Ordering {
                self.hash().cmp(&other.hash())
            }
        }
        impl PartialOrd for #name {
            fn partial_cmp(&self, other: &#name) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }
        impl PartialEq for #name {
            fn eq(&self, other: &#name) -> bool {
                self.hash() == other.hash()
            }
        }
        impl Eq for #name {}
    };
    gen.into()
}

/// This macro will provide a hasable implementation to the a given struct
#[proc_macro_derive(Hashable, attributes(Digest, hash))]
pub fn derive_hashable(tokens: TokenStream) -> TokenStream {
    let input = parse_macro_input!(tokens as DeriveInput);
    let object_name = &input.ident;
    let mut digest = None;
    for attr in &input.attrs {
        match attr.interpret_meta().unwrap() {
            syn::Meta::NameValue(val) => {
                if val.ident.to_string() == "Digest" {
                    if let syn::Lit::Str(lit) = &val.lit {
                        digest = Some(lit.value());
                    }
                }
            },
            _ => (),
        };
    }
    let item = input.data;
    let fields_text = match item {
        Data::Struct(ref item) => {
            match item.fields {
                Fields::Named(ref fields) => {
                    let recurse = fields.named.iter().map(|f| {
                        let mut do_we_ignore_field = false;
                        for attr in &f.attrs {
                            match attr.interpret_meta().unwrap() {
                                syn::Meta::NameValue(ref val) => {
                                    if val.ident.to_string() == "hash" {
                                        if let syn::Lit::Str(lit) = &val.lit {
                                            if lit.value() == "Hashable_ignore" {
                                                do_we_ignore_field = true;
                                            }
                                        }
                                    }
                                },
                                syn::Meta::List(ref val) => {
                                    // we have more than one property
                                    if val.ident.to_string() == "hash" {
                                        // we have a hash command here, lets search for the sub command
                                        for nestedmeta in val.nested.iter() {
                                            if let syn::NestedMeta::Meta(meta) = nestedmeta {
                                                if let syn::Meta::Word(ref val) = meta {
                                                    if val.to_string() == "Hashable_ignore" {
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
                                hasher.input((&self.#name).as_bytes());
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
                            hasher.input((&self.#index).as_bytes());
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
        Data::Enum(_) | Data::Union(_) => unimplemented!(),
    };

    let digest = digest.expect("Could not find Digest attribute");
    let varname = Ident::new(&digest, Span::call_site());
    let gen = quote! {
        impl  Hashable for #object_name  {
            fn hash(&self) -> Vec<u8> {
                let mut hasher = <#varname>::new();
                #fields_text
                hasher.result().to_vec()
            }
        }
    };
    gen.into()
}
