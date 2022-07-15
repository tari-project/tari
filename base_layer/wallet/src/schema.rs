// Copyright 2020. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

table! {
    client_key_values (key) {
        key -> Text,
        value -> Text,
    }
}

table! {
    completed_transactions (tx_id) {
        tx_id -> BigInt,
        source_public_key -> Binary,
        destination_public_key -> Binary,
        amount -> BigInt,
        fee -> BigInt,
        transaction_protocol -> Text,
        status -> Integer,
        message -> Text,
        timestamp -> Timestamp,
        cancelled -> Nullable<Integer>,
        direction -> Nullable<Integer>,
        coinbase_block_height -> Nullable<BigInt>,
        send_count -> Integer,
        last_send_timestamp -> Nullable<Timestamp>,
        confirmations -> Nullable<BigInt>,
        mined_height -> Nullable<BigInt>,
        mined_in_block -> Nullable<Binary>,
        transaction_signature_nonce -> Binary,
        transaction_signature_key -> Binary,
    }
}

table! {
    contacts (public_key) {
        public_key -> Binary,
        node_id -> Binary,
        alias -> Text,
        last_seen -> Nullable<Timestamp>,
        latency -> Nullable<Integer>,
    }
}

table! {
    inbound_transactions (tx_id) {
        tx_id -> BigInt,
        source_public_key -> Binary,
        amount -> BigInt,
        receiver_protocol -> Text,
        message -> Text,
        timestamp -> Timestamp,
        cancelled -> Integer,
        direct_send_success -> Integer,
        send_count -> Integer,
        last_send_timestamp -> Nullable<Timestamp>,
    }
}

table! {
    key_manager_states (id) {
        id -> Integer,
        branch_seed -> Text,
        primary_key_index -> Binary,
        timestamp -> Timestamp,
    }
}

table! {
    key_manager_states_old (id) {
        id -> Integer,
        seed -> Binary,
        branch_seed -> Text,
        primary_key_index -> BigInt,
        timestamp -> Timestamp,
    }
}

table! {
    known_one_sided_payment_scripts (script_hash) {
        script_hash -> Binary,
        private_key -> Binary,
        script -> Binary,
        input -> Binary,
        script_lock_height -> BigInt,
    }
}

table! {
    outbound_transactions (tx_id) {
        tx_id -> BigInt,
        destination_public_key -> Binary,
        amount -> BigInt,
        fee -> BigInt,
        sender_protocol -> Text,
        message -> Text,
        timestamp -> Timestamp,
        cancelled -> Integer,
        direct_send_success -> Integer,
        send_count -> Integer,
        last_send_timestamp -> Nullable<Timestamp>,
    }
}

table! {
    outputs (id) {
        id -> Integer,
        commitment -> Nullable<Binary>,
        spending_key -> Binary,
        value -> BigInt,
        output_type -> Integer,
        maturity -> BigInt,
        recovery_byte -> Integer,
        status -> Integer,
        hash -> Nullable<Binary>,
        script -> Binary,
        input_data -> Binary,
        script_private_key -> Binary,
        script_lock_height -> BigInt,
        sender_offset_public_key -> Binary,
        metadata_signature_nonce -> Binary,
        metadata_signature_u_key -> Binary,
        metadata_signature_v_key -> Binary,
        mined_height -> Nullable<BigInt>,
        mined_in_block -> Nullable<Binary>,
        mined_mmr_position -> Nullable<BigInt>,
        marked_deleted_at_height -> Nullable<BigInt>,
        marked_deleted_in_block -> Nullable<Binary>,
        received_in_tx_id -> Nullable<BigInt>,
        spent_in_tx_id -> Nullable<BigInt>,
        coinbase_block_height -> Nullable<BigInt>,
        metadata -> Nullable<Binary>,
        features_parent_public_key -> Nullable<Binary>,
        features_unique_id -> Nullable<Binary>,
        features_json -> Text,
        spending_priority -> Integer,
        covenant -> Binary,
        encrypted_value -> Binary,
        contract_id -> Nullable<Binary>,
        minimum_value_promise -> BigInt,
    }
}

table! {
    scanned_blocks (header_hash) {
        header_hash -> Binary,
        height -> BigInt,
        num_outputs -> Nullable<BigInt>,
        amount -> Nullable<BigInt>,
        timestamp -> Timestamp,
    }
}

table! {
    wallet_settings (key) {
        key -> Text,
        value -> Text,
    }
}

allow_tables_to_appear_in_same_query!(
    client_key_values,
    completed_transactions,
    contacts,
    inbound_transactions,
    key_manager_states,
    key_manager_states_old,
    known_one_sided_payment_scripts,
    outbound_transactions,
    outputs,
    scanned_blocks,
    wallet_settings,
);
