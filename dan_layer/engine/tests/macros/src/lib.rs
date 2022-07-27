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
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    parse2,
    ItemImpl,
    ItemStruct,
    Result,
};

#[allow(dead_code)]
struct TemplateAst {
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

pub fn generate_template(input: TokenStream) -> Result<TokenStream> {
    let _template_ast = parse2::<TemplateAst>(input).unwrap();

    let output = quote! {
        pub mod template {}
    };

    Ok(output)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use proc_macro2::TokenStream;

    use crate::generate_template;

    #[test]
    fn test_hello() {
        let input = TokenStream::from_str(
            "struct MockTemplate { value: u32 } impl MockTemplate { pub fn foo() -> u32 { 1_u32 } }",
        )
        .unwrap();

        let output = generate_template(input).unwrap();
        println!("{}", output);
    }
}
