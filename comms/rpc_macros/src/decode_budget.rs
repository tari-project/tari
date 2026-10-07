// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! `#[derive(DecodeBudget)]`: generates a decode-budget walker for a prost message, oneof or enum from the
//! `#[prost(...)]` attributes prost-build emits.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, GenericArgument, PathArguments, Type};

/// What a `#[prost(...)]` field attribute says about the field
enum FieldKind {
    /// A message field (optional, repeated or required) with this tag
    Message(u32),
    /// A map field with this tag whose values are messages
    MessageMap(u32),
    /// A map field with this tag whose values are scalars, bytes or strings
    ScalarMap(u32),
    /// A oneof field covering these tags
    Oneof(Vec<u32>),
    /// A `repeated bytes` or `repeated string` field with this tag: each element is charged, never entered
    RepeatedBytes(u32),
    /// A repeated scalar or enum field with this tag: a packed occurrence is charged its byte length
    RepeatedScalar(u32),
    /// Anything else (single scalars, bytes, strings, enums, maps of scalars): never entered
    Other,
}

pub fn expand(input: &DeriveInput) -> TokenStream {
    match expand_inner(input) {
        Ok(tokens) => tokens,
        Err(err) => err.to_compile_error(),
    }
}

fn expand_inner(input: &DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let module = quote!(::tari_comms::decode_budget);

    let body = match &input.data {
        Data::Struct(data) => {
            let mut arms = Vec::new();
            let mut map_tags = Vec::new();
            for field in &data.fields {
                match field_kind(&field.attrs)? {
                    FieldKind::Message(tag) => {
                        let ty = inner_type(&field.ty);
                        arms.push(quote!(#tag => budget.charge_message::<#ty>(contents),));
                    },
                    FieldKind::MessageMap(tag) => {
                        let ty = map_value_type(&field.ty)?;
                        map_tags.push(tag);
                        arms.push(quote!(#tag => budget.charge_map_entry::<#ty>(contents),));
                    },
                    FieldKind::ScalarMap(tag) => {
                        map_tags.push(tag);
                        arms.push(quote!(#tag => budget.charge_items(1),));
                    },
                    FieldKind::Oneof(tags) => {
                        let ty = inner_type(&field.ty);
                        arms.push(quote! {
                            #(#tags)|* => <#ty as #module::DecodeBudget>::count_oneof_field(tag, contents, budget),
                        });
                    },
                    FieldKind::RepeatedBytes(tag) => {
                        arms.push(quote!(#tag => budget.charge_items(1),));
                    },
                    FieldKind::RepeatedScalar(tag) => {
                        arms.push(quote!(#tag => budget.charge_items(contents.len()),));
                    },
                    FieldKind::Other => {},
                }
            }
            if arms.is_empty() {
                TokenStream::new()
            } else {
                quote! {
                    // Not every arm uses `contents`
                    #[allow(unused_variables)]
                    fn count_messages(
                        buf: &[u8],
                        budget: &mut #module::Budget,
                    ) -> ::std::result::Result<(), #module::DecodeBudgetExceeded> {
                        #module::walk_fields(buf, budget, &[#(#map_tags),*], |tag, contents, budget| match tag {
                            #(#arms)*
                            _ => ::std::result::Result::Ok(()),
                        })
                    }
                }
            }
        },
        Data::Enum(data) => {
            // A oneof: each variant holds one field. A plain protobuf enum has unit variants and no attributes that
            // name a message, so it counts nothing.
            let mut arms = Vec::new();
            for variant in &data.variants {
                if let FieldKind::Message(tag) = field_kind(&variant.attrs)? {
                    let ty = match &variant.fields {
                        Fields::Unnamed(fields) if fields.unnamed.len() == 1 => fields.unnamed.first().map(|f| &f.ty),
                        _ => None,
                    }
                    .ok_or_else(|| syn::Error::new_spanned(variant, "a oneof message variant must hold one field"))?;
                    let ty = inner_type(ty);
                    arms.push(quote!(#tag => budget.charge_message::<#ty>(contents),));
                }
            }
            if arms.is_empty() {
                TokenStream::new()
            } else {
                quote! {
                    fn count_oneof_field(
                        tag: u32,
                        contents: &[u8],
                        budget: &mut #module::Budget,
                    ) -> ::std::result::Result<(), #module::DecodeBudgetExceeded> {
                        match tag {
                            #(#arms)*
                            _ => ::std::result::Result::Ok(()),
                        }
                    }
                }
            }
        },
        Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                name,
                "DecodeBudget cannot be derived for a union",
            ));
        },
    };

    Ok(quote! {
        impl #impl_generics #module::DecodeBudget for #name #ty_generics #where_clause {
            #body
        }
    })
}

/// Reads the `#[prost(...)]` attribute(s) of a field or oneof variant.
fn field_kind(attrs: &[syn::Attribute]) -> syn::Result<FieldKind> {
    let mut is_message = false;
    let mut is_oneof = false;
    let mut is_bytes = false;
    let mut is_scalar = false;
    let mut is_repeated = false;
    let mut is_map = false;
    let mut map_of_messages = false;
    let mut tag = None;
    let mut tags = Vec::new();

    for attr in attrs.iter().filter(|attr| attr.path().is_ident("prost")) {
        attr.parse_nested_meta(|meta| {
            let name = meta.path.get_ident().map(ToString::to_string).unwrap_or_default();
            // Values are string or integer literals; a few prost options take a parenthesised list, which is skipped
            let value = if meta.input.peek(syn::Token![=]) {
                Some(meta.value()?.parse::<syn::Lit>()?)
            } else {
                if meta.input.peek(syn::token::Paren) {
                    meta.input.parse::<proc_macro2::Group>()?;
                }
                None
            };
            match (name.as_str(), value) {
                ("message", _) => is_message = true,
                ("bytes" | "string", _) => is_bytes = true,
                ("repeated", _) => is_repeated = true,
                (
                    "int32" | "int64" | "uint32" | "uint64" | "sint32" | "sint64" | "fixed32" | "fixed64" |
                    "sfixed32" | "sfixed64" | "bool" | "float" | "double" | "enumeration",
                    _,
                ) => is_scalar = true,
                ("oneof", _) => is_oneof = true,
                ("map", Some(syn::Lit::Str(spec))) => {
                    is_map = true;
                    map_of_messages = spec.value().split(',').nth(1).map(str::trim) == Some("message");
                },
                ("tag", Some(lit)) => tag = Some(parse_tag(&lit)?),
                ("tags", Some(syn::Lit::Str(list))) => {
                    for part in list.value().split(',') {
                        let part = part.trim();
                        tags.push(
                            part.parse::<u32>()
                                .map_err(|_| syn::Error::new_spanned(&list, format!("invalid tag `{part}`")))?,
                        );
                    }
                },
                _ => {},
            }
            Ok(())
        })?;
    }

    Ok(match tag {
        _ if is_oneof => FieldKind::Oneof(tags),
        Some(tag) if is_message => FieldKind::Message(tag),
        Some(tag) if map_of_messages => FieldKind::MessageMap(tag),
        Some(tag) if is_map => FieldKind::ScalarMap(tag),
        Some(tag) if is_repeated && is_bytes => FieldKind::RepeatedBytes(tag),
        Some(tag) if is_repeated && is_scalar => FieldKind::RepeatedScalar(tag),
        _ => FieldKind::Other,
    })
}

fn parse_tag(lit: &syn::Lit) -> syn::Result<u32> {
    match lit {
        syn::Lit::Str(s) => s
            .value()
            .trim()
            .parse()
            .map_err(|_| syn::Error::new_spanned(lit, "invalid tag")),
        syn::Lit::Int(i) => i.base10_parse(),
        _ => Err(syn::Error::new_spanned(lit, "invalid tag")),
    }
}

/// Strips `Option<_>`, `Box<_>` and `Vec<_>` wrappers: `Option<Box<T>>` and `Vec<T>` give `T`.
fn inner_type(ty: &Type) -> &Type {
    match last_segment_args(ty) {
        Some((ident, args)) if ident == "Option" || ident == "Box" || ident == "Vec" => match args.first() {
            Some(inner) => inner_type(inner),
            None => ty,
        },
        _ => ty,
    }
}

/// The value type `V` of a `HashMap<K, V>` / `BTreeMap<K, V>` field.
fn map_value_type(ty: &Type) -> syn::Result<&Type> {
    match last_segment_args(ty) {
        Some((_, args)) if args.len() == 2 => args
            .get(1)
            .map(|value| inner_type(value))
            .ok_or_else(|| syn::Error::new_spanned(ty, "expected a map type with two type arguments")),
        _ => Err(syn::Error::new_spanned(
            ty,
            "expected a map type with two type arguments",
        )),
    }
}

/// The last path segment's name and type arguments, e.g. `("Option", [T])` for `::core::option::Option<T>`.
fn last_segment_args(ty: &Type) -> Option<(&syn::Ident, Vec<&Type>)> {
    let Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    let args = match &segment.arguments {
        PathArguments::AngleBracketed(args) => args
            .args
            .iter()
            .filter_map(|arg| match arg {
                GenericArgument::Type(ty) => Some(ty),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    Some((&segment.ident, args))
}
