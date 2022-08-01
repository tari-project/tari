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
use quote::{format_ident, quote};
use syn::{parse_quote, Expr, Result};

use crate::ast::{FunctionAst, TemplateAst};

pub fn generate_abi(ast: &TemplateAst) -> Result<TokenStream> {
    let abi_function_name = format_ident!("{}_abi", ast.struct_section.ident);
    let template_name_as_str = ast.template_name.to_string();
    let function_defs: Vec<Expr> = ast.get_functions().iter().map(generate_function_def).collect();

    let output = quote! {
        #[no_mangle]
        pub extern "C" fn #abi_function_name() -> *mut u8 {
            use ::tari_template_abi::{encode_with_len, FunctionDef, TemplateDef, Type};

            let template = TemplateDef {
                template_name: #template_name_as_str.to_string(),
                functions: vec![ #(#function_defs),* ],
            };

            let buf = encode_with_len(&template);
            wrap_ptr(buf)
        }
    };

    Ok(output)
}

fn generate_function_def(f: &FunctionAst) -> Expr {
    let name = f.name.clone();
    let arguments: Vec<Expr> = f
        .input_types
        .iter()
        .map(String::as_str)
        .map(generate_abi_type)
        .collect();
    let output = generate_abi_type(&f.output_type);

    parse_quote!(
        FunctionDef {
            name: #name.to_string(),
            arguments: vec![ #(#arguments),* ],
            output: #output,
        }
    )
}

fn generate_abi_type(rust_type: &str) -> Expr {
    // TODO: there may be a better way of handling this
    match rust_type {
        "" => parse_quote!(Type::Unit),
        "bool" => parse_quote!(Type::Bool),
        "i8" => parse_quote!(Type::I8),
        "i16" => parse_quote!(Type::I16),
        "i32" => parse_quote!(Type::I32),
        "i64" => parse_quote!(Type::I64),
        "i128" => parse_quote!(Type::I128),
        "u8" => parse_quote!(Type::U8),
        "u16" => parse_quote!(Type::U16),
        "u32" => parse_quote!(Type::U32),
        "u64" => parse_quote!(Type::U64),
        "u128" => parse_quote!(Type::U128),
        "String" => parse_quote!(Type::String),
        _ => todo!(),
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use indoc::indoc;
    use proc_macro2::TokenStream;
    use quote::quote;
    use syn::parse2;

    use super::generate_abi;
    use crate::ast::TemplateAst;

    #[test]
    fn test_hello_world() {
        let input = TokenStream::from_str(indoc! {"
            mod foo {
                struct Foo {}
                impl Foo {
                    pub fn no_args_function() -> String {
                        \"Hello World!\".to_string()
                    }
                    pub fn some_args_function(a: i8, b: String) -> u32 {
                        1_u32
                    }
                    pub fn no_return_function() {}  
                } 
            }
        "})
        .unwrap();

        let ast = parse2::<TemplateAst>(input).unwrap();

        let output = generate_abi(&ast).unwrap();

        assert_code_eq(output, quote! {
            #[no_mangle]
            pub extern "C" fn Foo_abi() -> *mut u8 {
                use ::tari_template_abi::{encode_with_len, FunctionDef, TemplateDef, Type};

                let template = TemplateDef {
                    template_name: "Foo".to_string(),
                    functions: vec![
                        FunctionDef {
                            name: "no_args_function".to_string(),
                            arguments: vec![],
                            output: Type::String,
                        },
                        FunctionDef {
                            name: "some_args_function".to_string(),
                            arguments: vec![Type::I8, Type::String],
                            output: Type::U32,
                        },
                        FunctionDef {
                            name: "no_return_function".to_string(),
                            arguments: vec![],
                            output: Type::Unit,
                        }
                    ],
                };

                let buf = encode_with_len(&template);
                wrap_ptr(buf)
            }
        });
    }

    fn assert_code_eq(a: TokenStream, b: TokenStream) {
        assert_eq!(a.to_string(), b.to_string());
    }
}
