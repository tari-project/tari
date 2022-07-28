use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::Result;

use crate::ast::TemplateAst;

pub fn generate_dispatcher(ast: &TemplateAst) -> Result<TokenStream> {
    let function_name = format_ident!("{}_main", ast.struct_section.ident);

    let output = quote! {
        #[no_mangle]
        pub extern "C" fn #function_name(call_info: *mut u8, call_info_len: usize) -> *mut u8 {
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
