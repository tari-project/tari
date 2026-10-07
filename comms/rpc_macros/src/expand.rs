//  Copyright 2020, The Tari Project
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

use std::collections::HashMap;

use proc_macro2::TokenStream;
use syn::{FnArg, GenericArgument, ItemTrait, PathArguments, ReturnType, TraitItem, TraitItemFn, Type};

use crate::{generator::RpcCodeGenerator, method_info::RpcMethodInfo, options::RpcTraitOptions};

/// Expands `#[tari_rpc(...)]` on `node`. Any problem with the trait is returned as a compile error.
pub fn expand_trait(mut node: ItemTrait, options: RpcTraitOptions) -> TokenStream {
    let rpc_methods = match collect_rpc_methods(&mut node, &options) {
        Ok(methods) => methods,
        Err(err) => return err.to_compile_error(),
    };
    let generator = RpcCodeGenerator::new(options, node.ident.clone(), rpc_methods);
    let rpc_code = generator.generate();
    quote::quote! {
        #[::tari_comms::async_trait]
        #node
        #rpc_code
    }
}

/// Removes the `#[rpc(...)]` attribute from every RPC method of the trait and returns what was learned about each.
/// All errors found are returned together.
fn collect_rpc_methods(node: &mut ItemTrait, options: &RpcTraitOptions) -> syn::Result<Vec<RpcMethodInfo>> {
    let mut methods = Vec::new();
    let mut errors = Vec::new();

    for item in &mut node.items {
        let TraitItem::Fn(method) = item else {
            continue;
        };
        let mut rpc_attrs = Vec::new();
        method.attrs.retain(|attr| {
            if attr.path().is_ident("rpc") {
                rpc_attrs.push(attr.clone());
                false
            } else {
                true
            }
        });
        let Some(attr) = rpc_attrs.first() else {
            continue;
        };
        if let Some(extra) = rpc_attrs.get(1) {
            errors.push(syn_error!(
                extra,
                "only one #[rpc(...)] attribute is allowed per method"
            ));
            continue;
        }
        match parse_rpc_method(attr, method) {
            Ok(info) => methods.push(info),
            Err(err) => errors.push(err),
        }
    }

    // Method number checks run once every method is known, so they do not depend on the order of the attributes
    let mut seen = HashMap::new();
    for method in &methods {
        if method.method_num == 0 {
            errors.push(syn_error!(
                &method.method_lit,
                "method must be greater than 0 in #[rpc(...)] attribute for method `{}`",
                method.method_ident
            ));
        }
        if options
            .reserved_methods
            .iter()
            .any(|lit| lit.base10_parse::<u32>().ok() == Some(method.method_num))
        {
            errors.push(syn_error!(
                &method.method_lit,
                "method number `{}` is reserved and must not be used (method `{}`)",
                method.method_num,
                method.method_ident
            ));
        }
        if let Some(first) = seen.insert(method.method_num, &method.method_ident) {
            errors.push(syn_error!(
                &method.method_lit,
                "duplicate method number `{}` in #[rpc(...)] attribute (already used by `{}`)",
                method.method_num,
                first
            ));
        }
    }

    match errors.into_iter().reduce(|mut all, err| {
        all.combine(err);
        all
    }) {
        Some(err) => Err(err),
        None => Ok(methods),
    }
}

fn parse_rpc_method(attr: &syn::Attribute, method: &TraitItemFn) -> syn::Result<RpcMethodInfo> {
    let mut method_lit = None;
    let mut max_items = None;
    let mut max_request_items = None;
    attr.parse_nested_meta(|meta| {
        let ident = meta
            .path
            .get_ident()
            .ok_or_else(|| {
                meta.error("expected `method = <number>`, `max_items = <number>` or `max_request_items = <number>`")
            })?
            .clone();
        let lit: syn::LitInt = meta.value()?.parse()?;
        match ident.to_string().as_str() {
            "method" => {
                if method_lit.is_some() {
                    return Err(syn_error!(ident, "`method` is specified more than once"));
                }
                lit.base10_parse::<u32>()?;
                method_lit = Some(lit);
            },
            "max_items" => {
                if max_items.is_some() {
                    return Err(syn_error!(ident, "`max_items` is specified more than once"));
                }
                let value = lit.base10_parse::<usize>()?;
                if value == 0 {
                    return Err(syn_error!(lit, "max_items must be greater than 0"));
                }
                max_items = Some(value);
            },
            "max_request_items" => {
                if max_request_items.is_some() {
                    return Err(syn_error!(ident, "`max_request_items` is specified more than once"));
                }
                let value = lit.base10_parse::<usize>()?;
                if value == 0 {
                    return Err(syn_error!(lit, "max_request_items must be greater than 0"));
                }
                max_request_items = Some(value);
            },
            s => return Err(syn_error!(ident, "invalid option `{}` in #[rpc(...)] attribute", s)),
        }
        Ok(())
    })?;

    let method_lit = method_lit.ok_or_else(|| {
        syn_error!(
            attr,
            "#[rpc(...)] on method `{}` requires `method = <number>`",
            method.sig.ident
        )
    })?;
    let method_num = method_lit.base10_parse::<u32>()?;

    check_receiver(method)?;
    let request_type = parse_request_type(method)?;
    let (is_server_streaming, return_type) = parse_return_type(method)?;

    Ok(RpcMethodInfo {
        method_ident: method.sig.ident.clone(),
        method_lit,
        method_num,
        max_items,
        max_request_items,
        is_server_streaming,
        request_type,
        return_type,
    })
}

/// Checks the method is `fn name(&self, request: Request<_>)`.
fn check_receiver(method: &TraitItemFn) -> syn::Result<()> {
    let sig = &method.sig;
    match sig.inputs.first() {
        Some(FnArg::Receiver(receiver)) if receiver.reference.is_some() && receiver.mutability.is_none() => {},
        Some(arg) => return Err(syn_error!(arg, "the first argument of an RPC method must be `&self`")),
        None => return Err(syn_error!(sig, "RPC method `{}` has no arguments", sig.ident)),
    }
    if sig.inputs.len() != 2 {
        return Err(syn_error!(
            &sig.inputs,
            "RPC methods must take exactly 2 arguments i.e `&self` and `request: Request<_>`"
        ));
    }
    Ok(())
}

/// Returns the last segment of a path type, e.g. `Request<T>` for `rpc::Request<T>`.
fn last_segment(ty: &Type) -> Option<&syn::PathSegment> {
    match ty {
        Type::Path(path) if path.qself.is_none() => path.path.segments.last(),
        _ => None,
    }
}

/// Returns the type arguments of `segment`, e.g. `[A, B]` for `Result<A, B>`. Returns `None` if any generic argument
/// is not a type.
fn type_args(segment: &syn::PathSegment) -> Option<Vec<&Type>> {
    match &segment.arguments {
        PathArguments::AngleBracketed(args) => args
            .args
            .iter()
            .map(|arg| match arg {
                GenericArgument::Type(ty) => Some(ty),
                _ => None,
            })
            .collect(),
        _ => None,
    }
}

/// Returns `T` if `ty` is `<name><T>`.
fn single_type_arg<'a>(ty: &'a Type, name: &str) -> Option<&'a Type> {
    let segment = last_segment(ty).filter(|segment| segment.ident == name)?;
    match type_args(segment)?.as_slice() {
        [inner] => Some(*inner),
        _ => None,
    }
}

/// Returns `T` in the `request: Request<T>` argument.
fn parse_request_type(method: &TraitItemFn) -> syn::Result<Type> {
    let arg = method
        .sig
        .inputs
        .iter()
        .nth(1)
        .ok_or_else(|| syn_error!(&method.sig, "expected a `request: Request<_>` argument"))?;
    match arg {
        FnArg::Typed(pat_type) => single_type_arg(&pat_type.ty, "Request")
            .cloned()
            .ok_or_else(|| syn_error!(&pat_type.ty, "the request argument must be of type `Request<_>`")),
        FnArg::Receiver(_) => Err(syn_error!(arg, "expected a `request: Request<_>` argument")),
    }
}

/// Returns whether the method streams its response and `T` in its `Result<Response<T> | Streaming<T>, RpcStatus>`
/// return type.
fn parse_return_type(method: &TraitItemFn) -> syn::Result<(bool, Type)> {
    let invalid = || {
        syn_error!(
            &method.sig.output,
            "method `{}` has an invalid return type. Expected `Result<Response<_>, RpcStatus>` or \
             `Result<Streaming<_>, RpcStatus>`",
            method.sig.ident
        )
    };
    let ReturnType::Type(_, ty) = &method.sig.output else {
        return Err(invalid());
    };
    let segment = last_segment(ty).filter(|segment| segment.ident == "Result");
    let args = segment.and_then(type_args).ok_or_else(invalid)?;
    let [ok, err] = args.as_slice() else {
        return Err(invalid());
    };
    if !last_segment(err).is_some_and(|segment| segment.ident == "RpcStatus" && segment.arguments.is_none()) {
        return Err(syn_error!(err, "the error type of an RPC method must be `RpcStatus`"));
    }
    if let Some(inner) = single_type_arg(ok, "Response") {
        return Ok((false, inner.clone()));
    }
    if let Some(inner) = single_type_arg(ok, "Streaming") {
        return Ok((true, inner.clone()));
    }
    Err(invalid())
}
