use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    token::Comma,
    FnArg,
    Ident,
    ImplItem,
    ItemImpl,
    ItemStruct,
    Result,
    ReturnType,
};

#[allow(dead_code)]
pub struct TemplateAst {
    pub template_name: Ident,
    pub struct_section: ItemStruct,
    pub impl_section: ItemImpl,
}

impl Parse for TemplateAst {
    fn parse(input: ParseStream) -> Result<Self> {
        let struct_section: ItemStruct = input.parse()?;
        let impl_section = input.parse()?;
        let template_name = struct_section.ident.clone();

        Ok(Self {
            template_name,
            struct_section,
            impl_section,
        })
    }
}

impl TemplateAst {
    pub fn get_functions(&self) -> Vec<FunctionAst> {
        self.impl_section
            .items
            .iter()
            .map(Self::get_function_from_item)
            .collect()
    }

    fn get_function_from_item(item: &ImplItem) -> FunctionAst {
        match item {
            ImplItem::Method(m) => FunctionAst {
                name: m.sig.ident.to_string(),
                input_types: Self::get_input_type_tokens(&m.sig.inputs),
                output_type: Self::get_output_type_token(&m.sig.output),
            },
            _ => todo!(),
        }
    }

    fn get_input_type_tokens(inputs: &Punctuated<FnArg, Comma>) -> Vec<String> {
        inputs
            .iter()
            .map(|arg| match arg {
                // TODO: handle the "self" case
                syn::FnArg::Receiver(_) => todo!(),
                syn::FnArg::Typed(t) => Self::get_type_token(&t.ty),
            })
            .collect()
    }

    fn get_output_type_token(ast_type: &ReturnType) -> String {
        match ast_type {
            syn::ReturnType::Default => String::new(), // the function does not return anything
            syn::ReturnType::Type(_, t) => Self::get_type_token(t),
        }
    }

    fn get_type_token(syn_type: &syn::Type) -> String {
        match syn_type {
            syn::Type::Path(type_path) => {
                // TODO: handle "Self"
                // TODO: detect more complex types
                type_path.path.segments[0].ident.to_string()
            },
            _ => todo!(),
        }
    }
}

pub struct FunctionAst {
    pub name: String,
    pub input_types: Vec<String>,
    pub output_type: String,
}
