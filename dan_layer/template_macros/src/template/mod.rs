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

    let dependencies = generate_dependencies();
    let definition = generate_definition(&ast);
    let abi = generate_abi(&ast)?;
    let dispatcher = generate_dispatcher(&ast)?;

    let output = quote! {
        #dependencies

        #definition

        #abi

        #dispatcher
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
                    pub fn set(&mut self, value: u32) {
                        self.value = value;
                    }
                } 
            }
        "})
        .unwrap();

        let output = generate_template(input).unwrap();

        assert_code_eq(output, quote! {
            use tari_template_lib::template_dependencies::*;

            pub mod template {
                use super::*;

                #[derive(Decode, Encode)]
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
                    pub fn set(&mut self, value: u32) {
                        self.value = value;
                    }
                }
            }

            #[no_mangle]
            pub extern "C" fn State_abi() -> *mut u8 {
                use ::tari_template_abi::{encode_with_len, FunctionDef, TemplateDef, Type, wrap_ptr};

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
                            output: Type::Unit,
                        }
                    ],
                };

                let buf = encode_with_len(&template);
                wrap_ptr(buf)
            }

            #[no_mangle]
            pub extern "C" fn State_main(call_info: *mut u8, call_info_len: usize) -> *mut u8 {
                use ::tari_template_abi::{decode, encode_with_len, CallInfo, wrap_ptr};
                use ::tari_template_lib::set_context_from_call_info;

                if call_info.is_null() {
                    panic!("call_info is null");
                }

                let call_data = unsafe { Vec::from_raw_parts(call_info, call_info_len, call_info_len) };
                let call_info: CallInfo = decode(&call_data).unwrap();

                set_context_from_call_info(&call_info);
                engine().emit_log(LogLevel::Debug, format!("Dispatcher called with function {}" , call_info.func_name));

                let result;
                match call_info.func_name.as_str() {
                    "new" => {
                        let state = template::State::new();
                        let rtn = engine().instantiate("State".to_string(), state);
                        result = encode_with_len(&rtn);
                    },
                    "get" => {
                        let component = decode::<::tari_template_lib::models::ComponentInstance>(&call_info.args[0usize]).unwrap();
                        let mut state = decode::<template::State>(&component.state).unwrap();
                        let rtn = template::State::get(&mut state);
                        result = encode_with_len(&rtn);
                    },
                    "set" => {
                        let component = decode::<::tari_template_lib::models::ComponentInstance>(&call_info.args[0usize]).unwrap();
                        let mut state = decode::<template::State>(&component.state).unwrap();
                        let arg_1 = decode::<u32>(&call_info.args[1usize]).unwrap();
                        let rtn = template::State::set(&mut state, arg_1);
                        result = encode_with_len(&rtn);
                        engine().set_component_state(component.id(), state);
                    },
                    _ => panic!("invalid function name")
                };

                wrap_ptr(result)
            }
        });
    }

    fn assert_code_eq(a: TokenStream, b: TokenStream) {
        assert_eq!(a.to_string(), b.to_string());
    }
}
