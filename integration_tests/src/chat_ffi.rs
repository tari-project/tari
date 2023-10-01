//   Copyright 2023. The Tari Project
//
//   Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//   following conditions are met:
//
//   1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//   disclaimer.
//
//   2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//   following disclaimer in the documentation and/or other materials provided with the distribution.
//
//   3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//   products derived from this software without specific prior written permission.
//
//   THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//   INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//   DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//   SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//   SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//   WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//   USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    convert::TryFrom,
    ffi::{c_void, CString},
    path::PathBuf,
    str::FromStr,
    sync::{Arc, Mutex, Once},
};

use async_trait::async_trait;

type ClientFFI = c_void;

use libc::{c_char, c_int, c_uchar, c_uint};
use minotari_app_utilities::identity_management::setup_node_identity;
use tari_chat_client::{database, ChatClient};
use tari_common_types::tari_address::TariAddress;
use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{Peer, PeerFeatures},
    NodeIdentity,
};
use tari_contacts::contacts_service::{
    service::ContactOnlineStatus,
    types::{Message, MessageMetadataType},
};

use crate::{chat_client::test_config, get_port};

extern "C" fn callback_contact_status_change(_state: *mut c_void) {
    let callback = ChatCallback::instance();
    *callback.contact_status_change.lock().unwrap() += 1;
}

extern "C" fn callback_message_received(_state: *mut c_void) {
    let callback = ChatCallback::instance();
    *callback.message_received.lock().unwrap() += 1;
}

#[cfg_attr(windows, link(name = "minotari_chat_ffi.dll"))]
#[cfg_attr(not(windows), link(name = "minotari_chat_ffi"))]
extern "C" {
    pub fn create_chat_client(
        config: *mut c_void,
        error_out: *const c_int,
        callback_contact_status_change: unsafe extern "C" fn(*mut c_void),
        callback_message_received: unsafe extern "C" fn(*mut c_void),
    ) -> *mut ClientFFI;
    pub fn create_chat_message(receiver: *mut c_void, message: *const c_char, error_out: *const c_int) -> *mut c_void;
    pub fn send_chat_message(client: *mut ClientFFI, message: *mut c_void, error_out: *const c_int);
    pub fn add_chat_message_metadata(
        message: *mut c_void,
        metadata_type: c_int,
        data: *const c_char,
        error_out: *const c_int,
    ) -> *mut c_void;
    pub fn add_chat_contact(client: *mut ClientFFI, address: *mut c_void, error_out: *const c_int);
    pub fn check_online_status(client: *mut ClientFFI, address: *mut c_void, error_out: *const c_int) -> c_int;
    pub fn get_chat_messages(
        client: *mut ClientFFI,
        sender: *mut c_void,
        limit: c_int,
        page: c_int,
        error_out: *const c_int,
    ) -> *mut c_void;
    pub fn destroy_chat_client_ffi(client: *mut ClientFFI);
    pub fn chat_byte_vector_create(
        byte_array: *const c_uchar,
        element_count: c_uint,
        error_our: *const c_int,
    ) -> *mut c_void;
}

#[derive(Debug)]
pub struct PtrWrapper(*mut ClientFFI);
unsafe impl Send for PtrWrapper {}

#[derive(Debug)]
pub struct ChatFFI {
    ptr: Arc<Mutex<PtrWrapper>>,
    pub identity: Arc<NodeIdentity>,
}

#[async_trait]
impl ChatClient for ChatFFI {
    async fn add_contact(&self, address: &TariAddress) {
        let client = self.ptr.lock().unwrap();

        let address_ptr = Box::into_raw(Box::new(address.to_owned())) as *mut c_void;

        let error_out = Box::into_raw(Box::new(0));
        unsafe { add_chat_contact(client.0, address_ptr, error_out) }
    }

    async fn check_online_status(&self, address: &TariAddress) -> ContactOnlineStatus {
        let client = self.ptr.lock().unwrap();

        let address_ptr = Box::into_raw(Box::new(address.clone())) as *mut c_void;

        let result;
        let error_out = Box::into_raw(Box::new(0));
        unsafe { result = check_online_status(client.0, address_ptr, error_out) }

        ContactOnlineStatus::from_byte(u8::try_from(result).unwrap()).expect("A valid u8 from FFI status")
    }

    async fn send_message(&self, message: Message) {
        let client = self.ptr.lock().unwrap();

        let error_out = Box::into_raw(Box::new(0));
        let message_ptr = Box::into_raw(Box::new(message)) as *mut c_void;

        unsafe {
            send_chat_message(client.0, message_ptr, error_out);
        }
    }

    async fn get_messages(&self, address: &TariAddress, limit: u64, page: u64) -> Vec<Message> {
        let client = self.ptr.lock().unwrap();

        let address_ptr = Box::into_raw(Box::new(address.clone())) as *mut c_void;

        let messages;
        unsafe {
            let error_out = Box::into_raw(Box::new(0));
            let limit = i32::try_from(limit).expect("Truncation occurred") as c_int;
            let page = i32::try_from(page).expect("Truncation occurred") as c_int;
            let all_messages = get_chat_messages(client.0, address_ptr, limit, page, error_out) as *mut Vec<Message>;
            messages = (*all_messages).clone();
        }

        messages
    }

    fn create_message(&self, receiver: &TariAddress, message: String) -> Message {
        let address_ptr = Box::into_raw(Box::new(receiver.to_owned())) as *mut c_void;

        let message_c_str = CString::new(message).unwrap();
        let message_c_char: *const c_char = CString::into_raw(message_c_str) as *const c_char;

        let error_out = Box::into_raw(Box::new(0));

        unsafe {
            let message_ptr = create_chat_message(address_ptr, message_c_char, error_out) as *mut Message;
            *Box::from_raw(message_ptr)
        }
    }

    fn add_metadata(&self, message: Message, metadata_type: MessageMetadataType, data: String) -> Message {
        let message_ptr = Box::into_raw(Box::new(message)) as *mut c_void;
        let message_type = metadata_type.as_byte();

        let error_out = Box::into_raw(Box::new(0));

        let bytes = data.into_bytes();
        let len = i32::try_from(bytes.len()).expect("Truncation occurred") as c_uint;
        let byte_data = unsafe { chat_byte_vector_create(bytes.as_ptr(), len, error_out) };

        unsafe {
            add_chat_message_metadata(
                message_ptr,
                i32::from(message_type),
                byte_data as *const c_char,
                error_out,
            );
            *Box::from_raw(message_ptr as *mut Message)
        }
    }

    async fn send_read_receipt(&self, _address: &TariAddress, _message_id: Vec<u8>) {
        todo!();
    }

    fn identity(&self) -> &NodeIdentity {
        &self.identity
    }

    fn shutdown(&mut self) {
        let client = self.ptr.lock().unwrap();

        unsafe { destroy_chat_client_ffi(client.0) }
    }
}

pub async fn spawn_ffi_chat_client(name: &str, seed_peers: Vec<Peer>, base_dir: PathBuf) -> ChatFFI {
    let port = get_port(18000..18499).unwrap();
    let address = Multiaddr::from_str(&format!("/ip4/127.0.0.1/tcp/{}", port)).unwrap();

    let base_dir = base_dir
        .join("ffi_chat_clients")
        .join(format!("{}_port_{}", name, port));

    let mut config = test_config(address.clone());
    config.chat_client.set_base_path(base_dir);

    let identity = setup_node_identity(
        &config.chat_client.identity_file,
        vec![address],
        true,
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();

    database::create_chat_storage(&config.chat_client.db_file);
    database::create_peer_storage(&config.chat_client.data_dir);

    config.peer_seeds.peer_seeds = seed_peers
        .iter()
        .map(|p| p.to_short_string())
        .collect::<Vec<String>>()
        .into();

    let config_ptr = Box::into_raw(Box::new(config)) as *mut c_void;

    let client_ptr;

    let error_out = Box::into_raw(Box::new(0));

    unsafe {
        *ChatCallback::instance().contact_status_change.lock().unwrap() = 0;

        client_ptr = create_chat_client(
            config_ptr,
            error_out,
            callback_contact_status_change,
            callback_message_received,
        );
    }

    ChatFFI {
        ptr: Arc::new(Mutex::new(PtrWrapper(client_ptr))),
        identity,
    }
}

static mut INSTANCE: Option<ChatCallback> = None;
static START: Once = Once::new();

#[derive(Default)]
pub struct ChatCallback {
    pub contact_status_change: Mutex<u64>,
    pub message_received: Mutex<u64>,
}

impl ChatCallback {
    pub fn instance() -> &'static mut Self {
        unsafe {
            START.call_once(|| {
                INSTANCE = Some(Self::default());
            });
            INSTANCE.as_mut().unwrap()
        }
    }
}
