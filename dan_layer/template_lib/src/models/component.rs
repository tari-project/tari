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

// TODO: use the actual component id type
pub type ComponentId = ([u8; 32], u32);

use tari_template_abi::{Decode, Encode, encode_with_len, ops::OP_CREATE_COMPONENT, CreateComponentArg};

use crate::call_engine;

pub fn initialise<T: Encode + Decode>(template_name: String, initial_state: T) -> ComponentId {
    let encoded_state = encode_with_len(&initial_state);

    // Call the engine to create a new component
    // TODO: proper component id
    // TODO: what happens if the user wants to return multiple components/types?
    let component_id = call_engine::<_, ComponentId>(OP_CREATE_COMPONENT, &CreateComponentArg {
        name: template_name,
        quantity: 1,
        metadata: Default::default(),
        state: encoded_state,
    });
    component_id.expect("no asset id returned")
}

pub fn get_state<T: Encode + Decode>(_id: u32) -> T {
    // get the component state
    // TODO: use a real op code (not "123") when they are implemented
    let _state = call_engine::<_, ()>(123, &());

    // create and return a mock state because state is not implemented yet in the engine
    let len = std::mem::size_of::<T>();
    let byte_vec = vec![0_u8; len];
    let mut mock_value = byte_vec.as_slice();
    T::deserialize(&mut mock_value).unwrap()
}

pub fn set_state<T: Encode + Decode>(_id: u32, _state: T) {
    // update the component value
    // TODO: use a real op code (not "123") when they are implemented
    call_engine::<_, ()>(123, &());
}
