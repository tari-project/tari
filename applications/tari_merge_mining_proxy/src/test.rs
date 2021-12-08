//  Copyright 2020, The Tari Project
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

mod add_aux_data {
    use serde_json::json;

    use crate::{
        common::json_rpc,
        proxy::{add_aux_data, MMPROXY_AUX_KEY_NAME},
    };

    #[test]
    fn it_adds_aux_data() {
        let v = json_rpc::success_response(None, json!({ "hello": "world"}));
        let v = add_aux_data(v, json!({"test": "works"}));
        assert_eq!(v["result"][MMPROXY_AUX_KEY_NAME]["test"].as_str().unwrap(), "works");
    }

    #[test]
    fn it_merges_to_existing_aux_data() {
        let v = json_rpc::success_response(None, json!({ "hello": "world"}));
        let v = add_aux_data(v, json!({"test1": 1}));
        let v = add_aux_data(v, json!({"test2": 2, "test3": 3}));
        assert_eq!(v["result"][MMPROXY_AUX_KEY_NAME]["test1"].as_u64().unwrap(), 1);
        assert_eq!(v["result"][MMPROXY_AUX_KEY_NAME]["test2"].as_u64().unwrap(), 2);
        assert_eq!(v["result"][MMPROXY_AUX_KEY_NAME]["test3"].as_u64().unwrap(), 3);
    }

    #[test]
    fn it_does_not_add_data_to_errors() {
        let v = json_rpc::error_response(None, 1, "it's on 🔥", None);
        let v = add_aux_data(v, json!({"it": "is broken"}));
        assert!(v["result"][MMPROXY_AUX_KEY_NAME]["it"].as_str().is_none());
    }
}

mod append_aux_chain_data {
    use serde_json::json;

    use crate::{
        common::json_rpc,
        proxy::{append_aux_chain_data, MMPROXY_AUX_KEY_NAME},
    };

    #[test]
    fn it_adds_a_chain_object() {
        let v = json_rpc::success_response(None, json!({}));
        let v = append_aux_chain_data(v, json!({"test": "works"}));
        assert_eq!(v["result"][MMPROXY_AUX_KEY_NAME]["chains"].as_array().unwrap(), &[
            json!({"test": "works"})
        ]);
    }
}
