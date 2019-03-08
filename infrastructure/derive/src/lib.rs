#![recursion_limit = "128"]

extern crate proc_macro;
extern crate syn;
#[macro_use]
extern crate quote;

use proc_macro::TokenStream;

// This macro will produce the 4 trait implementations required for an hashable struct to be sorted
#[proc_macro_derive(HashableOrdering)]
pub fn derive_hashable_ordering(tokens: TokenStream) -> TokenStream {
    // Parse TokenStream into AST
    let ast: syn::DeriveInput = syn::parse(tokens).unwrap();
    let name = &ast.ident;
    let gen = quote! {
         impl Ord for #name {
            fn cmp(&self, other: &#name) -> Ordering {
                self.hash().cmp(&other.hash())
            }
        }
        impl PartialOrd for #name {
            fn partial_cmp(&self, other: &#name) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }
        impl PartialEq for #name {
            fn eq(&self, other: &#name) -> bool {
                self.hash() == other.hash()
            }
        }
        impl Eq for #name {}
    };
    gen.into()
}
