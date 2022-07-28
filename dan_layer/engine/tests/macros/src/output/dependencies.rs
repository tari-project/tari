use proc_macro2::TokenStream;
use quote::quote;

pub fn generate_dependencies() -> TokenStream {
    quote! {
        extern "C" {
            pub fn tari_engine(op: u32, input_ptr: *const u8, input_len: usize) -> *mut u8;
        }

        pub fn wrap_ptr(mut v: Vec<u8>) -> *mut u8 {
            use std::mem;

            let ptr = v.as_mut_ptr();
            mem::forget(v);
            ptr
        }

        #[no_mangle]
        pub unsafe extern "C" fn tari_alloc(len: u32) -> *mut u8 {
            use std::{mem, intrinsics::copy};

            let cap = (len + 4) as usize;
            let mut buf = Vec::<u8>::with_capacity(cap);
            let ptr = buf.as_mut_ptr();
            mem::forget(buf);
            copy(len.to_le_bytes().as_ptr(), ptr, 4);
            ptr
        }

        #[no_mangle]
        pub unsafe extern "C" fn tari_free(ptr: *mut u8) {
            use std::intrinsics::copy;

            let mut len = [0u8; 4];
            copy(ptr, len.as_mut_ptr(), 4);

            let cap = (u32::from_le_bytes(len) + 4) as usize;
            let _ = Vec::<u8>::from_raw_parts(ptr, cap, cap);
        }
    }
}
