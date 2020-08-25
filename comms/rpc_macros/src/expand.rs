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

use crate::{generator::RpcCodeGenerator, method_info::RpcMethodInfo, options::RpcTraitOptions};
use quote::ToTokens;
use syn::{
    export::TokenStream2,
    fold,
    fold::Fold,
    FnArg,
    GenericArgument,
    ItemTrait,
    Meta,
    NestedMeta,
    PathArguments,
    ReturnType,
    Type,
};

pub fn expand_trait(node: ItemTrait, options: RpcTraitOptions) -> TokenStream2 {
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
    fn is_rpc_method(&self, node: &syn::TraitItemMethod) -> bool {
        node.attrs.iter().position(|at| at.path.is_ident("rpc")).is_some()
    }

    fn parse_trait_item_method(&mut self, node: &mut syn::TraitItemMethod) -> syn::Result<RpcMethodInfo> {
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

    fn parse_attr(&self, node: &mut syn::TraitItemMethod, info: &mut RpcMethodInfo) -> syn::Result<()> {
        let attr = node
            .attrs
            .iter()
            .position(|at| at.path.is_ident("rpc"))
            .map(|pos| node.attrs.remove(pos))
            .ok_or_else(|| {
                let ident = node.sig.ident.to_string();
                syn_error!(node, "Missing #[rpc(...)] attribute on method `{}`", ident)
            })?;

        let meta = attr.parse_meta().unwrap();
        match meta {
            Meta::List(meta_list) => {
                for meta in meta_list.nested {
                    match meta {
                        NestedMeta::Meta(meta) => match meta {
                            Meta::NameValue(name_value) => {
                                let ident = name_value
                                    .path
                                    .get_ident()
                                    .expect("Invalid syntax for #[rpc(...)] attribute");
                                match ident.to_string().as_str() {
                                    "method" => {
                                        info.method_num = extract_u32(&ident, &name_value.lit)?;
                                        self.validate_method_num(ident, info.method_num)?;
                                        if info.method_num == 0 {
                                            return Err(syn_error!(
                                                name_value,
                                                "method must be greater than 0 in `#[rpc(...)]` attribute for method \
                                                 `{}`",
                                                info.method_ident,
                                            ));
                                        }
                                    },
                                    s => {
                                        return Err(syn_error!(
                                            name_value,
                                            "Invalid option `{}` in #[rpc(...)] attribute",
                                            s
                                        ))
                                    },
                                }
                            },
                            m => {
                                return Err(syn_error!(
                                    m,
                                    "Invalid syntax given to #[rpc(...)] attribute. Expected a name/value pair.",
                                ))
                            },
                        },
                        m => {
                            return Err(syn_error!(
                                m,
                                "Invalid syntax given to #[rpc(...)] attribute. Expected a name/value pair",
                            ))
                        },
                    }
                }
            },
            m => {
                return Err(syn_error!(
                    m,
                    "Invalid syntax given to #[rpc(...)] attribute. Expected a name/value pair",
                ))
            },
        }

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

    fn parse_method_signature(&self, node: &syn::TraitItemMethod, info: &mut RpcMethodInfo) -> syn::Result<()> {
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
                                },
                                _ => return Err(syn_error!(request_arg, "expected request type")),
                            }
                        },
                        _ => return Err(syn_error!(request_arg, "expected request type")),
                    }
                },
                _ => return Err(syn_error!(request_arg, "expected request type")),
            },
            _ => return Err(syn_error!(request_arg, "expected request argument, got a receiver")),
        }

        let ident = info.method_ident.clone();
        let invalid_return_type = || {
            syn_error!(
                &node.sig.output,
                "Method `{}` has an invalid return type. Expected: `Result<_, RpcStatus>`",
                ident
            )
        };

        match &node.sig.output {
            ReturnType::Default => {
                return Err(invalid_return_type());
            },
            ReturnType::Type(_, ty) => match &**ty {
                Type::Path(path) => match path.path.segments.first() {
                    Some(syn::PathSegment {
                        arguments: syn::PathArguments::AngleBracketed(args),
                        ..
                    }) => {
                        let arg = args.args.first().ok_or_else(invalid_return_type)?;
                        match arg {
                            GenericArgument::Type(ty) => match ty {
                                Type::Path(syn::TypePath { path, .. }) => {
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
                                                },
                                                _ => return Err(invalid_return_type()),
                                            }
                                        },
                                        _ => return Err(invalid_return_type()),
                                    }
                                },
                                _ => return Err(invalid_return_type()),
                            },
                            _ => return Err(invalid_return_type()),
                        }
                    },
                    _ => return Err(invalid_return_type()),
                },
                _ => {
                    return Err(invalid_return_type());
                },
            },
        }

        Ok(())
    }
}

impl Fold for TraitInfoCollector {
    fn fold_item_trait(&mut self, node: syn::ItemTrait) -> syn::ItemTrait {
        self.trait_ident = Some(node.ident.clone());
        fold::fold_item_trait(self, node)
    }

    fn fold_trait_item_method(&mut self, mut node: syn::TraitItemMethod) -> syn::TraitItemMethod {
        if self.is_rpc_method(&node) {
            let info = match self.parse_trait_item_method(&mut node) {
                Ok(i) => i,
                Err(err) => {
                    panic!("{}", err);
                },
            };

            self.rpc_methods.push(info);
        }

        fold::fold_trait_item_method(self, node)
    }
}

fn extract_u32(ident: &syn::Ident, lit: &syn::Lit) -> syn::Result<u32> {
    match lit {
        syn::Lit::Int(int) => int.base10_parse(),
        l => Err(syn_error!(
            ident,
            "Expected integer for `{}` in the #[rpc(...)] attribute, got {:?}",
            ident,
            l
        )),
    }
}
