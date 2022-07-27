mod template;

use proc_macro::TokenStream;

#[proc_macro]
pub fn template(input: TokenStream) -> TokenStream {
    template::generate_template(proc_macro2::TokenStream::from(input))
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}
