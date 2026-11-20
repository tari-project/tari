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
use syn::{
    Ident,
    LitByteStr,
    LitInt,
    Token,
    bracketed,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};

/// Options given to `#[tari_rpc(...)]`
#[derive(Debug)]
pub struct RpcTraitOptions {
    /// The protocol name used during protocol negotiation
    pub protocol_name: LitByteStr,
    /// The name of the generated client struct
    pub client_struct: Ident,
    /// The name of the generated server struct
    pub server_struct: Ident,
    /// Method numbers that must never be (re)used, e.g. the numbers of removed methods
    pub reserved_methods: Vec<LitInt>,
}

impl Parse for RpcTraitOptions {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut protocol_name = None;
        let mut server_struct = None;
        let mut client_struct = None;
        let mut reserved_methods = None;

        while !input.is_empty() {
            let name: Ident = input.parse()?;
            let _: Token![=] = input.parse()?;

            match name.to_string().as_str() {
                "protocol_name" => {
                    let lit: LitByteStr = input.parse()?;
                    if lit.value().is_empty() {
                        return Err(syn_error!(lit, "`protocol_name` must not be empty"));
                    }
                    set_once(&mut protocol_name, &name, lit)?;
                },
                "server_struct" => set_once(&mut server_struct, &name, input.parse()?)?,
                "client_struct" => set_once(&mut client_struct, &name, input.parse()?)?,
                "reserved_methods" => {
                    let content;
                    bracketed!(content in input);
                    let list = Punctuated::<LitInt, Token![,]>::parse_terminated(&content)?;
                    let mut seen = std::collections::HashSet::new();
                    for lit in &list {
                        let num = lit.base10_parse::<u32>()?;
                        if num == 0 {
                            return Err(syn_error!(
                                lit,
                                "method number in `reserved_methods` must be greater than 0"
                            ));
                        }
                        if !seen.insert(num) {
                            return Err(syn_error!(
                                lit,
                                "duplicate method number `{}` in `reserved_methods`",
                                num
                            ));
                        }
                    }
                    set_once(&mut reserved_methods, &name, list.into_iter().collect())?;
                },
                n => {
                    return Err(syn_error!(
                        name,
                        "expected `protocol_name`, `server_struct`, `client_struct` or `reserved_methods`, found `{}`",
                        n
                    ));
                },
            }

            if input.is_empty() {
                break;
            }
            let _: Token![,] = input.parse()?;
        }

        Ok(Self {
            protocol_name: protocol_name.ok_or_else(|| missing("protocol_name = b\"...\""))?,
            server_struct: server_struct.ok_or_else(|| missing("server_struct = <Name>"))?,
            client_struct: client_struct.ok_or_else(|| missing("client_struct = <Name>"))?,
            reserved_methods: reserved_methods.unwrap_or_default(),
        })
    }
}

/// Stores `value` in `slot`, or errors if the option was already given.
fn set_once<T>(slot: &mut Option<T>, name: &Ident, value: T) -> syn::Result<()> {
    if slot.is_some() {
        return Err(syn_error!(name, "`{}` is specified more than once", name));
    }
    *slot = Some(value);
    Ok(())
}

fn missing(option: &str) -> syn::Error {
    syn::Error::new(Span::call_site(), format!("#[tari_rpc(...)] requires `{option}`"))
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::*;

    #[test]
    fn rejects_empty_protocol_name() {
        let err = syn::parse2::<RpcTraitOptions>(quote!(protocol_name = b"", server_struct = Srv, client_struct = Cli))
            .unwrap_err();
        assert!(err.to_string().contains("`protocol_name` must not be empty"));
    }

    #[test]
    fn rejects_zero_or_duplicate_reserved_methods() {
        let err_zero = syn::parse2::<RpcTraitOptions>(quote!(
            protocol_name = b"/test/1",
            server_struct = Srv,
            client_struct = Cli,
            reserved_methods = [0]
        ))
        .unwrap_err();
        assert!(
            err_zero
                .to_string()
                .contains("method number in `reserved_methods` must be greater than 0")
        );

        let err_dup = syn::parse2::<RpcTraitOptions>(quote!(
            protocol_name = b"/test/1",
            server_struct = Srv,
            client_struct = Cli,
            reserved_methods = [2, 5, 2]
        ))
        .unwrap_err();
        assert!(
            err_dup
                .to_string()
                .contains("duplicate method number `2` in `reserved_methods`")
        );
    }
}
