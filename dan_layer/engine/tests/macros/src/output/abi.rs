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
