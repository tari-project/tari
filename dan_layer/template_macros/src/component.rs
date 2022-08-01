use tari_template_abi::{Decode, Encode};

#[allow(dead_code)]
pub type ComponentId = u32;

pub trait ComponentState: Encode + Decode {}

#[allow(dead_code)]
pub fn initialise<T: ComponentState>(_initial_state: T) -> ComponentId {
    // TODO: call the engine initialize the component
    // tari_engine(op: u32, input_ptr: *const u8, input_len: usize) -> *mut u8;

    0_u32
}

#[allow(dead_code)]
pub fn get_state<T: ComponentState>(_id: ComponentId) -> T {
    // TODO: call the engine to get the state
    // tari_engine(op: u32, input_ptr: *const u8, input_len: usize) -> *mut u8;

    let len = std::mem::size_of::<T>();
    let byte_vec = vec![0_u8; len];
    let mut value = byte_vec.as_slice();
    T::deserialize(&mut value).unwrap()
}

#[allow(dead_code)]
pub fn set_state<T: ComponentState>(_id: ComponentId, _state: T) {
    // TODO: call the engine to set the state
    // tari_engine(op: u32, input_ptr: *const u8, input_len: usize) -> *mut u8;
}
