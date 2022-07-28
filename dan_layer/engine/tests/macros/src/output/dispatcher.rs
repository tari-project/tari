use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{token::Brace, Block, Expr, ExprBlock, Result};

use crate::ast::TemplateAst;

pub fn generate_dispatcher(ast: &TemplateAst) -> Result<TokenStream> {
    let dispatcher_function_name = format_ident!("{}_main", ast.struct_section.ident);
    let function_names = get_function_names(ast);
    let function_blocks = get_function_blocks(ast);

    let output = quote! {
        #[no_mangle]
        pub extern "C" fn #dispatcher_function_name(call_info: *mut u8, call_info_len: usize) -> *mut u8 {
            use ::tari_template_abi::{decode, encode_with_len, CallInfo};

            if call_info.is_null() {
                panic!("call_info is null");
            }

            let call_data = unsafe { Vec::from_raw_parts(call_info, call_info_len, call_info_len) };
            let call_info: CallInfo = decode(&call_data).unwrap();

            let result = match call_info.func_name.as_str() {
                #( #function_names => #function_blocks )*,
                _ => panic!("invalid function name")
            };

            wrap_ptr(encode_with_len(&result))
        }
    };

    Ok(output)
}

pub fn get_function_names(ast: &TemplateAst) -> Vec<String> {
    ast.get_functions().iter().map(|f| f.name.clone()).collect()
}

pub fn get_function_blocks(ast: &TemplateAst) -> Vec<Expr> {
    let mut blocks = vec![];

    for function in ast.get_functions() {
        let statements = function.statements;
        blocks.push(Expr::Block(ExprBlock {
            attrs: vec![],
            label: None,
            block: Block {
                brace_token: Brace {
                    span: Span::call_site(),
                },
                stmts: statements,
            },
        }));
    }

    blocks
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use proc_macro2::TokenStream;
    use quote::quote;
    use syn::parse2;

    use crate::{ast::TemplateAst, output::dispatcher::generate_dispatcher};

    #[test]
    fn test_hello_world() {
        let input = TokenStream::from_str(
            "struct HelloWorld {} impl HelloWorld { pub fn greet() -> String { \"Hello World!\".to_string() } }",
        )
        .unwrap();

        let ast = parse2::<TemplateAst>(input).unwrap();

        let output = generate_dispatcher(&ast).unwrap();

        assert_code_eq(output, quote! {
            #[no_mangle]
            pub extern "C" fn HelloWorld_main(call_info: *mut u8, call_info_len: usize) -> *mut u8 {
                use ::tari_template_abi::{decode, encode_with_len, CallInfo};

                if call_info.is_null() {
                    panic!("call_info is null");
                }

                let call_data = unsafe { Vec::from_raw_parts(call_info, call_info_len, call_info_len) };
                let call_info: CallInfo = decode(&call_data).unwrap();

                let result = match call_info.func_name.as_str() {
                    "greet" => { "Hello World!".to_string() },
                    _ => panic!("invalid function name")
                };

                wrap_ptr(encode_with_len(&result))
            }
        });
    }

    fn assert_code_eq(a: TokenStream, b: TokenStream) {
        assert_eq!(a.to_string(), b.to_string());
    }
}
