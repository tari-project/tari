use syn::{
    parse::{Parse, ParseStream},
    ItemImpl,
    ItemStruct,
    Result,
};

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