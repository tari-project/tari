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

use crate::{method_info::RpcMethodInfo, options::RpcTraitOptions};
use quote::quote;
use syn::export::TokenStream2;

pub struct RpcCodeGenerator {
    options: RpcTraitOptions,
    trait_ident: syn::Ident,
    rpc_methods: Vec<RpcMethodInfo>,
}

impl RpcCodeGenerator {
    pub fn new(options: RpcTraitOptions, trait_ident: syn::Ident, rpc_methods: Vec<RpcMethodInfo>) -> Self {
        Self {
            options,
            trait_ident,
            rpc_methods,
        }
    }

    pub fn generate(self) -> TokenStream2 {
        let server_code = self.generate_server_code();
        let client_code = self.generate_client_code();

        quote! {
            #server_code
            #client_code
        }
    }

    fn generate_server_code(&self) -> TokenStream2 {
        let server_struct = self.options.server_struct.as_ref().unwrap();
        let trait_ident = &self.trait_ident;
        let protocol_name = &self.options.protocol_name;
        let dep_mod = quote!(tari_comms::protocol::rpc::__macro_reexports);

        let match_branches = self
            .rpc_methods
            .iter()
            .map(|m| {
                let method_num = m.method_num;
                let method_name = &m.method_ident;
                let ret = if m.is_server_streaming {
                    quote!(Ok(Response::new(resp.into_body())))
                } else {
                    quote!(Ok(resp.map(IntoBody::into_body)))
                };
                quote! {
                    #method_num => {
                         let fut = async move {
                            let resp = inner.#method_name(req.decode()?).await?;
                            #ret
                        };
                        Box::pin(fut)
                    },
                }
            })
            .collect::<TokenStream2>();

        let service_method_select_body = quote! {
            match req.method().id() {
                #match_branches

                id => Box::pin(#dep_mod::future::ready(Err(RpcStatus::unsupported_method(format!(
                    "Method identifier `{}` is not recognised or supported",
                    id
                ))))),
            }
        };

        quote::quote! {
            pub struct #server_struct<T> {
                inner: std::sync::Arc<T>,
            }

            impl<T: #trait_ident> #server_struct<T> {
                pub fn new(service: T) -> Self {
                    Self {
                        inner: std::sync::Arc::new(service),
                    }
                }
            }

            impl<T: #trait_ident> #dep_mod::Service<#dep_mod::Request<#dep_mod::Bytes>> for #server_struct<T> {
                type Error = #dep_mod::RpcStatus;
                type Future = #dep_mod::BoxFuture<'static, Result<#dep_mod::Response<#dep_mod::Body>, #dep_mod::RpcStatus>>;
                type Response = #dep_mod::Response<#dep_mod::Body>;

                fn poll_ready(&mut self, _: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
                    std::task::Poll::Ready(Ok(()))
                }

                fn call(&mut self, req: #dep_mod::Request<#dep_mod::Bytes>) -> Self::Future {
                    use #dep_mod::IntoBody;
                    let inner = self.inner.clone();
                    #service_method_select_body
                }
            }

            impl<T> Clone for #server_struct<T> {
                fn clone(&self) -> Self {
                    Self {
                        inner: self.inner.clone(),
                    }
                }
            }

            impl<T> #dep_mod::NamedProtocolService for #server_struct<T> {
                const PROTOCOL_NAME: &'static [u8] = #protocol_name;
            }

            /// A service maker for #server_struct
            impl<T> #dep_mod::Service<#dep_mod::ProtocolId> for #server_struct<T>
            where T: #trait_ident
            {
                type Error = #dep_mod::RpcError;
                type Response = Self;

                type Future = #dep_mod::future::Ready<Result<Self::Response, Self::Error>>;

                fn poll_ready(&mut self, _: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
                    std::task::Poll::Ready(Ok(()))
                }

                fn call(&mut self, _: #dep_mod::ProtocolId) -> Self::Future {
                    #dep_mod::future::ready(Ok(self.clone()))
                }
            }
        }
    }

    fn generate_client_code(&self) -> TokenStream2 {
        let client_struct = self.options.client_struct.as_ref().unwrap();
        let protocol_name = &self.options.protocol_name;
        let dep_mod = quote!(::tari_comms::protocol::rpc::__macro_reexports);

        let client_methods = self
            .rpc_methods
            .iter()
            .map(|m| {
                let name = &m.method_ident;
                let method_num = m.method_num;
                let request_type = &m.request_type;
                let result_type = &m.return_type;
                let is_unit = m.request_type.as_ref().filter(|ty| is_unit_type(*ty)).is_some();

                let var = if is_unit { quote!(()) } else { quote!(request) };

                let body = if m.is_server_streaming {
                    quote!(self.inner.server_streaming(#var, #method_num).await)
                } else {
                    quote!(self.inner.request_response(#var, #method_num).await)
                };

                let ok_type = if m.is_server_streaming {
                    quote!(#dep_mod::ClientStreaming<#result_type>)
                } else {
                    quote!(#result_type)
                };

                let params = if is_unit {
                    TokenStream2::new()
                } else {
                    quote!(request: #request_type)
                };

                quote! {
                    pub async fn #name(&mut self,#params) -> Result<#ok_type, #dep_mod::RpcError> {
                        #body
                    }
                }
            })
            .collect::<TokenStream2>();

        let client_struct_body = quote! {
            pub async fn connect<TSubstream>(framed: #dep_mod::CanonicalFraming<TSubstream>) -> Result<Self, #dep_mod::RpcError>
              where TSubstream: #dep_mod::AsyncRead + #dep_mod::AsyncWrite + Unpin + Send + 'static {
                let inner = #dep_mod::RpcClient::connect(Default::default(), framed).await?;
                Ok(Self { inner })
            }

            pub fn builder() -> #dep_mod::RpcClientBuilder<Self> {
                #dep_mod::RpcClientBuilder::new()
            }

            #client_methods

            pub async fn get_last_request_latency(&mut self) -> Result<Option<std::time::Duration>, #dep_mod::RpcError> {
                self.inner.get_last_request_latency().await
            }

            pub fn close(&mut self) {
                self.inner.close();
            }
        };

        quote! {
            #[derive(Debug, Clone)]
            pub struct #client_struct {
                inner: #dep_mod::RpcClient,
            }

            impl #dep_mod::NamedProtocolService for #client_struct {
                const PROTOCOL_NAME: &'static [u8] = #protocol_name;
            }

            impl #client_struct {
                #client_struct_body
            }

            impl From<#dep_mod::RpcClient> for #client_struct {
                fn from(inner: #dep_mod::RpcClient) -> Self {
                    Self { inner }
                }
            }
        }
    }
}

fn is_unit_type(ty: &syn::Type) -> bool {
    match ty {
        syn::Type::Tuple(tuple) => tuple.elems.is_empty(),
        _ => false,
    }
}
