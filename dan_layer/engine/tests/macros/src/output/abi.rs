use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_quote, Expr, Result};
use tari_template_abi::FunctionDef;

use crate::ast::TemplateAst;

pub fn generate_abi(ast: &TemplateAst) -> Result<TokenStream> {
    let template_name_str = format!("{}", ast.struct_section.ident);
    let function_name = format_ident!("{}_abi", ast.struct_section.ident);

    let function_defs = ast.get_function_definitions();
    let function_defs_output: Vec<Expr> = function_defs.iter().map(generate_function_def).collect();

    let output = quote! {
        #[no_mangle]
        pub extern "C" fn #function_name() -> *mut u8 {
            use ::tari_template_abi::{encode_with_len, FunctionDef, TemplateDef, Type};

            let template = TemplateDef {
                template_name: #template_name_str.to_string(),
                functions: vec![ #(#function_defs_output),* ],
            };

            let buf = encode_with_len(&template);
            wrap_ptr(buf)
        }
    };

    Ok(output)
}

fn generate_function_def(fd: &FunctionDef) -> Expr {
    let name = fd.name.clone();
    let arguments: Vec<Expr> = fd.arguments.iter().map(TemplateAst::get_abi_type_expr).collect();
    let output = TemplateAst::get_abi_type_expr(&fd.output);

    parse_quote!(
        FunctionDef {
            name: #name.to_string(),
            arguments: vec![ #(#arguments),* ],
            output: #output,
        }
    )
}
