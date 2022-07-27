//  Copyright 2022. The Tari Project
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
use quote::{quote, format_ident};
use syn::{
    parse2,
    Result,
};

use crate::ast::TemplateAst;

pub fn generate_template_output(input: TokenStream) -> Result<TokenStream> {
    let ast = parse2::<TemplateAst>(input).unwrap();

    let mod_output = generate_mod_output(&ast);
    let abi_output = generate_abi_output(&ast)?;
    let main_output = generate_main_output(&ast)?;
    let engine_output = generate_engine_output();

    let output = quote! {
        #mod_output

        #abi_output

        #main_output

        #engine_output
    };

    Ok(output)
}

fn generate_mod_output(ast: &TemplateAst) ->TokenStream {
    let template_name = format_ident!("{}", ast.struct_section.ident);
    let functions = &ast.impl_section.items;

    quote! {
        pub mod template {
            use super::*;

            pub struct #template_name {
                // TODO: fill template fields
            }

            impl #template_name {
                #(#functions)*
            }
        }
    }
}

fn generate_abi_output(ast: &TemplateAst) -> Result<TokenStream>  {
    let template_name_str = format!("{}", ast.struct_section.ident);
    let function_name = format_ident!("{}_abi",  ast.struct_section.ident);

    let output = quote! {
        #[no_mangle]
        pub extern "C" fn #function_name() -> *mut u8 {
            use ::common::wrap_ptr;
            use ::tari_template_abi::{encode_with_len, FunctionDef, TemplateDef, Type};

            let template = TemplateDef {
                template_name: #template_name_str.to_string(),
                functions: vec![FunctionDef {
                    name: "greet".to_string(),
                    arguments: vec![],
                    output: Type::String,
                }],
            };

            let buf = encode_with_len(&template);
            wrap_ptr(buf)
        }
    };

    Ok(output)
}

fn generate_main_output(ast: &TemplateAst) -> Result<TokenStream>  {
    let function_name = format_ident!("{}_main", ast.struct_section.ident);

    let output = quote! {
        #[no_mangle]
        pub extern "C" fn #function_name(call_info: *mut u8, call_info_len: usize) -> *mut u8 {
            use ::common::wrap_ptr;
            use ::tari_template_abi::{decode, encode_with_len, CallInfo};

            if call_info.is_null() {
                panic!("call_info is null");
            }

            let call_data = unsafe { Vec::from_raw_parts(call_info, call_info_len, call_info_len) };
            let call_info: CallInfo = decode(&call_data).unwrap();

            let result = match call_info.func_name.as_str() {
                "greet" => "Hello World!".to_string(),
                _ => panic!("invalid function name")
            };

            wrap_ptr(encode_with_len(&result))
        }
    };

    Ok(output)
}

fn generate_engine_output() -> TokenStream  {
    quote! {
        extern "C" {
            pub fn tari_engine(op: u32, input_ptr: *const u8, input_len: usize) -> *mut u8;
        } 
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use proc_macro2::TokenStream;

    use crate::template::generate_template_output;

    #[test]
    fn test_hello_world() {
        let input = TokenStream::from_str(
            "struct HelloWorld {} impl HelloWorld { pub fn greet() -> String { \"Hello World!\".to_string() } }",
        )
        .unwrap();

        let output = generate_template_output(input).unwrap();
        println!("{}", output);
    }
}
