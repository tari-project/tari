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

use proc_macro2::Span;
use quote::quote;
use syn::{
    parse::{Parse, ParseBuffer},
    Ident,
    Token,
};

#[derive(Debug)]
pub struct RpcTraitOptions {
    pub protocol_name: syn::LitByteStr,
    pub dep_module_name: Ident,
    pub client_struct: Option<Ident>,
    pub server_struct: Option<Ident>,
}
// Parses `= <value>` in `<name> = <value>` and returns value and span of name-value pair.
fn parse_value<T: Parse>(input: &ParseBuffer<'_>, name: &Ident) -> syn::Result<T> {
    if input.is_empty() {
        return Err(syn_error!(name, "expected `{0} = <identifier>`, found `{0}`", name));
    }
    let eq_token: Token![=] = input.parse()?;
    if input.is_empty() {
        let span = quote!(#name #eq_token);
        return Err(syn_error!(span, "expected `{0} = <identifier>`, found `{0} =`", name));
    }
    let value = input.parse()?;
    Ok(value)
}

impl Parse for RpcTraitOptions {
    fn parse(input: &ParseBuffer<'_>) -> Result<Self, syn::Error> {
        let mut protocol_name = None;
        let mut server_struct = None;
        let mut client_struct = None;
        let mut module_name = syn::Ident::new("__rpc_deps", Span::call_site());

        while !input.is_empty() {
            let name: syn::Ident = input.parse()?;

            match name.to_string().as_str() {
                "protocol_name" => protocol_name = Some(parse_value(input, &name)?),
                "dep_module" => {
                    module_name = parse_value(input, &name)?;
                },
                "server_struct" => {
                    server_struct = parse_value(input, &name)?;
                },

                "client_struct" => {
                    client_struct = parse_value(input, &name)?;
                },
                n => {
                    return Err(syn_error!(
                        name,
                        "expected `protocol_name`, `dep_module`, `server_struct` or `client_struct`, found `{}`",
                        n
                    ))
                },
            }

            if input.is_empty() {
                break;
            }

            let _: Token![,] = input.parse()?;
        }

        Ok(Self {
            protocol_name: protocol_name
                .ok_or_else(|| syn::Error::new(Span::call_site(), "protocol_name must be specified"))?,
            dep_module_name: module_name,
            client_struct,
            server_struct,
        })
    }
}
