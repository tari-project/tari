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

mod abi;
mod definition;
mod dependencies;
mod dispatcher;

use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse2, Result};

use self::{
    abi::generate_abi,
    definition::generate_definition,
    dependencies::generate_dependencies,
    dispatcher::generate_dispatcher,
};
use crate::ast::TemplateAst;

pub fn generate_template(input: TokenStream) -> Result<TokenStream> {
    let ast = parse2::<TemplateAst>(input).unwrap();

    let definition = generate_definition(&ast);
    let abi = generate_abi(&ast)?;
    let dispatcher = generate_dispatcher(&ast)?;
    let dependencies = generate_dependencies();

    let output = quote! {
        #definition

        #abi

        #dispatcher

        #dependencies
    };

    Ok(output)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use indoc::indoc;
    use proc_macro2::TokenStream;
    use quote::quote;

    use super::generate_template;

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_state() {
        let input = TokenStream::from_str(indoc! {"
            mod test {
                struct State {
                    value: u32
                }
                impl State {
                    pub fn new() -> Self {
                        Self { value: 0 }
                    }
                    pub fn get(&self) -> u32 {
                        self.value
                    }
                    pub fn set(&mut self, value: u32) -> u32 {
                        self.value = value;
                        value
                    }
                } 
            }
        "})
        .unwrap();

        let output = generate_template(input).unwrap();

        assert_code_eq(output, quote! {
            pub mod template {
                use super::*;
                use tari_template_abi::borsh;

                #[derive(tari_template_abi::borsh::BorshSerialize, tari_template_abi::borsh::BorshDeserialize)]
                pub struct State {
                    value: u32
                }

                impl State {
                    pub fn new() -> Self {
                        Self { value: 0 }
                    }
                    pub fn get(&self) -> u32 {
                        self.value
                    }
                    pub fn set(&mut self, value: u32) -> u32 {
                        self.value = value;
                        value
                    }
                }
            }

            #[no_mangle]
            pub extern "C" fn State_abi() -> *mut u8 {
                use ::tari_template_abi::{encode_with_len, FunctionDef, TemplateDef, Type};

                let template = TemplateDef {
                    template_name: "State".to_string(),
                    functions: vec![
                        FunctionDef {
                            name: "new".to_string(),
                            arguments: vec![],
                            output: Type::U32,
                        },
                        FunctionDef {
                            name: "get".to_string(),
                            arguments: vec![Type::U32],
                            output: Type::U32,
                        },
                        FunctionDef {
                            name: "set".to_string(),
                            arguments: vec![Type::U32, Type::U32],
                            output: Type::U32,
                        }
                    ],
                };

                let buf = encode_with_len(&template);
                wrap_ptr(buf)
            }

            #[no_mangle]
            pub extern "C" fn State_main(call_info: *mut u8, call_info_len: usize) -> *mut u8 {
                use ::tari_template_abi::{decode, encode_with_len, CallInfo};
                use ::tari_template_lib::models::{get_state, set_state, initialise};

                if call_info.is_null() {
                    panic!("call_info is null");
                }

                let call_data = unsafe { Vec::from_raw_parts(call_info, call_info_len, call_info_len) };
                let call_info: CallInfo = decode(&call_data).unwrap();

                let result;
                match call_info.func_name.as_str() {
                    "new" => {
                        let state = template::State::new();
                        result = initialise(state);
                    },
                    "get" => {
                        let arg_0 = decode::<u32>(&call_info.args[0usize]).unwrap();
                        let mut state: template::State = get_state(arg_0);
                        result = template::State::get(&mut state);
                    },
                    "set" => {
                        let arg_0 = decode::<u32>(&call_info.args[0usize]).unwrap();
                        let arg_1 = decode::<u32>(&call_info.args[1usize]).unwrap();
                        let mut state: template::State = get_state(arg_0);
                        result = template::State::set(&mut state, arg_1);
                        set_state(arg_0, state);
                    },
                    _ => panic!("invalid function name")
                };

                wrap_ptr(encode_with_len(&result))
            }

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
        });
    }

    fn assert_code_eq(a: TokenStream, b: TokenStream) {
        assert_eq!(a.to_string(), b.to_string());
    }
}
