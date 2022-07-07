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

table! {
    instructions (id) {
        id -> Integer,
        hash -> Binary,
        node_id -> Integer,
        template_id -> Integer,
        method -> Text,
        args -> Binary,
        sender -> Binary,
    }
}

table! {
    locked_qc (id) {
        id -> Integer,
        message_type -> Integer,
        view_number -> BigInt,
        node_hash -> Binary,
        signature -> Nullable<Binary>,
    }
}

table! {
    metadata (key) {
        key -> Binary,
        value -> Binary,
    }
}

table! {
    nodes (id) {
        id -> Integer,
        hash -> Binary,
        parent -> Binary,
        height -> Integer,
        is_committed -> Bool,
    }
}

table! {
    prepare_qc (id) {
        id -> Integer,
        message_type -> Integer,
        view_number -> BigInt,
        node_hash -> Binary,
        signature -> Nullable<Binary>,
    }
}

table! {
    state_keys (schema_name, key_name) {
        schema_name -> Text,
        key_name -> Binary,
        value -> Binary,
    }
}

table! {
    state_op_log (id) {
        id -> Integer,
        height -> BigInt,
        merkle_root -> Nullable<Binary>,
        operation -> Text,
        schema -> Text,
        key -> Binary,
        value -> Nullable<Binary>,
    }
}

table! {
    state_tree (id) {
        id -> Integer,
        version -> Integer,
        is_current -> Bool,
        data -> Binary,
    }
}

joinable!(instructions -> nodes (node_id));

allow_tables_to_appear_in_same_query!(
    instructions,
    locked_qc,
    metadata,
    nodes,
    prepare_qc,
    state_keys,
    state_op_log,
    state_tree,
);
