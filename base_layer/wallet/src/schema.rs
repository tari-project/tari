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
        flags -> Integer,
        maturity -> BigInt,
        status -> Integer,
        hash -> Nullable<Binary>,
        script -> Binary,
        input_data -> Binary,
        script_private_key -> Binary,
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
        script_lock_height -> BigInt,
        spending_priority -> Integer,
        features_json -> Text,
        covenant -> Binary,
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
    known_one_sided_payment_scripts,
    outbound_transactions,
    outputs,
    scanned_blocks,
    wallet_settings,
);
