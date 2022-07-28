use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::ast::TemplateAst;

pub fn generate_definition(ast: &TemplateAst) -> TokenStream {
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
