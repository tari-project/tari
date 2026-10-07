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

use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::{FnArg, GenericArgument, ItemTrait, PathArguments, ReturnType, Type, fold, fold::Fold};

use crate::{generator::RpcCodeGenerator, method_info::RpcMethodInfo, options::RpcTraitOptions};

pub fn expand_trait(node: ItemTrait, options: RpcTraitOptions) -> TokenStream {
    let mut collector = TraitInfoCollector::new();
    let trait_code = collector.fold_item_trait(node);
    let generator = RpcCodeGenerator::new(options, collector.expect_trait_ident(), collector.rpc_methods);
    let rpc_code = generator.generate();
    quote::quote! {
        #[::tari_comms::async_trait]
        #trait_code
        #rpc_code
    }
}

struct TraitInfoCollector {
    rpc_methods: Vec<RpcMethodInfo>,
    trait_ident: Option<syn::Ident>,
}

impl TraitInfoCollector {
    pub fn new() -> Self {
        Self {
            rpc_methods: Vec::new(),
            trait_ident: None,
        }
    }

    pub fn expect_trait_ident(&mut self) -> syn::Ident {
        self.trait_ident.take().unwrap()
    }

    /// Returns true if a method has the `#[rpc(...)]` attribute, otherwise false
    fn is_rpc_method(&self, node: &syn::TraitItemFn) -> bool {
        node.attrs.iter().any(|at| at.path().is_ident("rpc"))
    }

    fn parse_trait_item_method(&mut self, node: &mut syn::TraitItemFn) -> syn::Result<RpcMethodInfo> {
        let mut info = RpcMethodInfo {
            method_ident: node.sig.ident.clone(),
            method_num: 0,
            is_server_streaming: false,
            request_type: None,
            return_type: None,
        };

        self.parse_attr(node, &mut info)?;
        self.parse_method_signature(node, &mut info)?;

        Ok(info)
    }

    fn parse_attr(&self, node: &mut syn::TraitItemFn, info: &mut RpcMethodInfo) -> syn::Result<()> {
        let attr = node
            .attrs
            .iter()
            .position(|at| at.path().is_ident("rpc"))
            .map(|pos| node.attrs.remove(pos))
            .ok_or_else(|| {
                let ident = node.sig.ident.to_string();
                syn_error!(node, "Missing #[rpc(...)] attribute on method `{}`", ident)
            })?;

        attr.parse_nested_meta(|meta| {
            let ident = meta
                .path
                .get_ident()
                .ok_or_else(|| meta.error("Invalid syntax for #[rpc(...)] attribute"))?;
            match ident.to_string().as_str() {
                "method" => {
                    let lit: syn::LitInt = meta.value()?.parse()?;
                    info.method_num = lit.base10_parse()?;
                    self.validate_method_num(ident, info.method_num)?;
                    if info.method_num == 0 {
                        return Err(syn_error!(
                            lit,
                            "method must be greater than 0 in `#[rpc(...)]` attribute for method `{}`",
                            info.method_ident,
                        ));
                    }
                    Ok(())
                },
                s => Err(syn_error!(ident, "Invalid option `{}` in #[rpc(...)] attribute", s)),
            }
        })?;

        Ok(())
    }

    fn validate_method_num<T: ToTokens>(&self, span: T, method_num: u32) -> Result<(), syn::Error> {
        if self.rpc_methods.iter().any(|m| m.method_num == method_num) {
            return Err(syn_error!(
                span,
                "duplicate method number `{}` in #[rpc(...]] attribute",
                method_num
            ));
        }

        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn parse_method_signature(&self, node: &syn::TraitItemFn, info: &mut RpcMethodInfo) -> syn::Result<()> {
        info.method_ident = node.sig.ident.clone();

        // Check the self receiver
        let arg = node
            .sig
            .inputs
            .first()
            .ok_or_else(|| syn_error!(node, "RPC method `{}` has no arguments.", node.sig.ident))?;
        match arg {
            FnArg::Receiver(receiver) => {
                if receiver.mutability.is_some() {
                    return Err(syn_error!(receiver, "Method receiver must be an immutable reference",));
                }
            },
            _ => return Err(syn_error!(arg, "First argument is not a self receiver")),
        }

        if node.sig.inputs.len() != 2 {
            return Err(syn_error!(
                arg,
                "All RPC methods must take 2 arguments i.e `&self` and `request: Request<_>`.",
            ));
        }

        self.parse_request_type(node, info)?;
        self.parse_method_return_type(node, info)?;

        Ok(())
    }

    fn parse_method_return_type(&self, node: &syn::TraitItemFn, info: &mut RpcMethodInfo) -> syn::Result<()> {
        let ident = info.method_ident.clone();
        let invalid_return_type = || {
            syn_error!(
                &node.sig.output,
                "Method `{}` has an invalid return type. Expected: `Result<_, RpcStatus>`",
                ident
            )
        };

        match &node.sig.output {
            ReturnType::Default => Err(invalid_return_type()),
            ReturnType::Type(_, ty) => match &**ty {
                Type::Path(path) => match path.path.segments.first() {
                    Some(syn::PathSegment {
                        arguments: syn::PathArguments::AngleBracketed(args),
                        ..
                    }) => {
                        let arg = args.args.first().ok_or_else(invalid_return_type)?;
                        match arg {
                            GenericArgument::Type(Type::Path(syn::TypePath { path, .. })) => {
                                let ret_ty = path.segments.first().ok_or_else(invalid_return_type)?;
                                // Check if the response is streaming
                                match ret_ty.ident.to_string().as_str() {
                                    "Response" => {
                                        info.is_server_streaming = false;
                                    },
                                    "Streaming" => {
                                        info.is_server_streaming = true;
                                    },
                                    _ => return Err(invalid_return_type()),
                                }
                                // Store the return type
                                match &ret_ty.arguments {
                                    PathArguments::AngleBracketed(args) => {
                                        let arg = args.args.first().ok_or_else(invalid_return_type)?;
                                        match arg {
                                            GenericArgument::Type(ty) => {
                                                info.return_type = Some((*ty).clone());
                                                Ok(())
                                            },
                                            _ => Err(invalid_return_type()),
                                        }
                                    },
                                    _ => Err(invalid_return_type()),
                                }
                            },

                            _ => Err(invalid_return_type()),
                        }
                    },
                    _ => Err(invalid_return_type()),
                },
                _ => Err(invalid_return_type()),
            },
        }
    }

    fn parse_request_type(&self, node: &syn::TraitItemFn, info: &mut RpcMethodInfo) -> syn::Result<()> {
        let request_arg = &node.sig.inputs[1];
        match request_arg {
            FnArg::Typed(syn::PatType { ty, .. }) => match &**ty {
                Type::Path(syn::TypePath { path, .. }) => {
                    let path = path
                        .segments
                        .first()
                        .ok_or_else(|| syn_error!(request_arg, "unexpected type in trait definition"))?;

                    match &path.arguments {
                        PathArguments::AngleBracketed(args) => {
                            let arg = args
                                .args
                                .first()
                                .ok_or_else(|| syn_error!(request_arg, "expected Request<T>"))?;
                            match arg {
                                GenericArgument::Type(ty) => {
                                    info.request_type = Some((*ty).clone());
                                    Ok(())
                                },
                                _ => Err(syn_error!(request_arg, "expected request type")),
                            }
                        },
                        _ => Err(syn_error!(request_arg, "expected request type")),
                    }
                },
                _ => Err(syn_error!(request_arg, "expected request type")),
            },
            _ => Err(syn_error!(request_arg, "expected request argument, got a receiver")),
        }
    }
}

impl Fold for TraitInfoCollector {
    fn fold_item_trait(&mut self, node: syn::ItemTrait) -> syn::ItemTrait {
        self.trait_ident = Some(node.ident.clone());
        fold::fold_item_trait(self, node)
    }

    fn fold_trait_item_fn(&mut self, mut node: syn::TraitItemFn) -> syn::TraitItemFn {
        if self.is_rpc_method(&node) {
            let info = match self.parse_trait_item_method(&mut node) {
                Ok(i) => i,
                Err(err) => {
                    panic!("{}", err);
                },
            };

            self.rpc_methods.push(info);
        }

        fold::fold_trait_item_fn(self, node)
    }
}
