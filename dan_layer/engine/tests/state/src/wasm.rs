use std::{collections::HashMap, mem};

use tari_template_abi::{encode_with_len, FunctionDef, TemplateDef, CallInfo, decode};

pub fn generate_abi(template_name: String, functions: Vec<FunctionDef>) -> *mut u8 {
    let template = TemplateDef {
        template_name,
        functions,
    };

    let buf = encode_with_len(&template);
    wrap_ptr(buf)
}

type FunctionImpl = Box<dyn Fn(Vec<Vec<u8>>) -> Vec<u8>>;

pub struct TemplateImpl(HashMap<String, FunctionImpl>);

impl TemplateImpl {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn add_function(&mut self, name: String, implementation: FunctionImpl) {
        self.0.insert(name.clone(), implementation);
    }
}

pub fn generate_main(call_info: *mut u8, call_info_len: usize, template_impl: TemplateImpl) -> *mut u8 {
    if call_info.is_null() {
        panic!("call_info is null");
    }

    let call_data = unsafe { Vec::from_raw_parts(call_info, call_info_len, call_info_len) };
    let call_info: CallInfo = decode(&call_data).unwrap();

    // get the function
    let function = match template_impl.0.get(&call_info.func_name) {
        Some(f) => f.clone(),
        None => panic!("invalid function name"),
    };

    // call the function
    let result = function(call_info.args);

    // return the encoded results of the function call
    wrap_ptr(result)
}

pub fn wrap_ptr(mut v: Vec<u8>) -> *mut u8 {
    let ptr = v.as_mut_ptr();
    mem::forget(v);
    ptr
}