use syn::{
    parse::{Parse, ParseStream},
    parse_quote,
    Expr,
    ItemImpl,
    ItemStruct,
    Result,
};
use tari_template_abi::{FunctionDef, Type};

#[allow(dead_code)]
pub struct TemplateAst {
    pub struct_section: ItemStruct,
    pub impl_section: ItemImpl,
}

impl Parse for TemplateAst {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            struct_section: input.parse()?,
            impl_section: input.parse()?,
        })
    }
}

impl TemplateAst {
    pub fn get_function_definitions(&self) -> Vec<FunctionDef> {
        self.impl_section
            .items
            .iter()
            .map(|item| match item {
                syn::ImplItem::Method(m) => FunctionDef {
                    name: m.sig.ident.to_string(),
                    arguments: Self::map_abi_input_types(&m.sig.inputs),
                    output: Self::map_abi_return_type(&m.sig.output),
                },
                _ => todo!(),
            })
            .collect()
    }

    fn map_abi_input_types(inputs: &syn::punctuated::Punctuated<syn::FnArg, syn::token::Comma>) -> Vec<Type> {
        inputs
            .iter()
            .map(|input| {
                match input {
                    // TODO: handle the "self" case
                    syn::FnArg::Receiver(_) => todo!(),
                    syn::FnArg::Typed(t) => Self::map_to_abi_type(&t.ty),
                }
            })
            .collect()
    }

    fn map_abi_return_type(ast_return_type: &syn::ReturnType) -> Type {
        match ast_return_type {
            syn::ReturnType::Default => Type::Unit,
            syn::ReturnType::Type(_, t) => Self::map_to_abi_type(t),
        }
    }

    fn map_to_abi_type(ast_type: &syn::Type) -> Type {
        match ast_type {
            syn::Type::Path(type_path) => {
                // TODO: handle "Self"
                // TODO: detect more complex types
                let ident = type_path.path.segments[0].ident.to_string();
                // TODO: refactor to avoid these hardcoded string values
                match ident.as_str() {
                    "()" => Type::Unit,
                    "bool" => Type::Bool,
                    "i8" => Type::I8,
                    "i16" => Type::I16,
                    "i32" => Type::I32,
                    "i64" => Type::I64,
                    "i128" => Type::I128,
                    "u8" => Type::U8,
                    "u16" => Type::U16,
                    "u32" => Type::U32,
                    "u64" => Type::U64,
                    "u128" => Type::U128,
                    "String" => Type::String,
                    _ => todo!(),
                }
            },
            _ => todo!(),
        }
    }

    // TODO: this function probably should not be here
    pub fn get_abi_type_expr(abi_type: &Type) -> Expr {
        // TODO: there must be a better way of doing this...
        match *abi_type {
            Type::Unit => parse_quote!(Type::Unit),
            Type::Bool => parse_quote!(Type::Bool),
            Type::I8 => parse_quote!(Type::I8),
            Type::I16 => parse_quote!(Type::I16),
            Type::I32 => parse_quote!(Type::I32),
            Type::I64 => parse_quote!(Type::I64),
            Type::I128 => parse_quote!(Type::I128),
            Type::U8 => parse_quote!(Type::U8),
            Type::U16 => parse_quote!(Type::U16),
            Type::U32 => parse_quote!(Type::U32),
            Type::U64 => parse_quote!(Type::U64),
            Type::U128 => parse_quote!(Type::U128),
            Type::String => parse_quote!(Type::String),
        }
    }
}
