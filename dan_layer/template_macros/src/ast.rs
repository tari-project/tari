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

use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    token::Comma,
    Error,
    FnArg,
    Ident,
    ImplItem,
    ImplItemMethod,
    ItemImpl,
    ItemMod,
    ItemStruct,
    Result,
    ReturnType,
    Signature,
    Stmt,
};

#[allow(dead_code)]
pub struct TemplateAst {
    pub template_name: Ident,
    pub struct_section: ItemStruct,
    pub impl_section: ItemImpl,
}

impl Parse for TemplateAst {
    fn parse(input: ParseStream) -> Result<Self> {
        // parse the "mod" block
        let module: ItemMod = input.parse()?;

        // get the contents of the "mod" block
        let items = match module.content {
            Some((_, items)) => items,
            None => return Err(Error::new(module.ident.span(), "empty module")),
        };

        // there should be two items: the "struct" and the "impl" blocks
        if items.len() != 2 {
            return Err(Error::new(module.ident.span(), "invalid number of module sections"));
        }

        // get the "struct" block
        let struct_section = match &items[0] {
            syn::Item::Struct(struct_item) => struct_item.clone(),
            _ => return Err(Error::new(module.ident.span(), "the first section is not a 'struct'")),
        };

        // get the "impl" block
        let impl_section = match &items[1] {
            syn::Item::Impl(impl_item) => impl_item.clone(),
            _ => return Err(Error::new(module.ident.span(), "the second section is not an 'impl'")),
        };

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
                input_types: Self::get_input_types(&m.sig.inputs),
                output_type: Self::get_output_type_token(&m.sig.output),
                statements: Self::get_statements(m),
                is_constructor: Self::is_constructor(&m.sig),
            },
            _ => todo!(),
        }
    }

    fn get_input_types(inputs: &Punctuated<FnArg, Comma>) -> Vec<TypeAst> {
        inputs
            .iter()
            .map(|arg| match arg {
                // TODO: handle the "self" case
                syn::FnArg::Receiver(r) => {
                    // TODO: validate that it's indeed a reference ("&") to self

                    let mutability = r.mutability.is_some();
                    TypeAst::Receiver { mutability }
                },
                syn::FnArg::Typed(t) => Self::get_type_ast(&t.ty),
            })
            .collect()
    }

    fn get_output_type_token(ast_type: &ReturnType) -> Option<TypeAst> {
        match ast_type {
            syn::ReturnType::Default => None, // the function does not return anything
            syn::ReturnType::Type(_, t) => Some(Self::get_type_ast(t)),
        }
    }

    fn get_type_ast(syn_type: &syn::Type) -> TypeAst {
        match syn_type {
            syn::Type::Path(type_path) => {
                // TODO: handle "Self"
                // TODO: detect more complex types
                TypeAst::Typed(type_path.path.segments[0].ident.clone())
            },
            _ => todo!(),
        }
    }

    fn get_statements(method: &ImplItemMethod) -> Vec<Stmt> {
        method.block.stmts.clone()
    }

    fn is_constructor(sig: &Signature) -> bool {
        match &sig.output {
            syn::ReturnType::Default => false, // the function does not return anything
            syn::ReturnType::Type(_, t) => match t.as_ref() {
                syn::Type::Path(type_path) => type_path.path.segments[0].ident == "Self",
                _ => false,
            },
        }
    }
}

pub struct FunctionAst {
    pub name: String,
    pub input_types: Vec<TypeAst>,
    pub output_type: Option<TypeAst>,
    pub statements: Vec<Stmt>,
    pub is_constructor: bool,
}

pub enum TypeAst {
    Receiver { mutability: bool },
    Typed(Ident),
}
