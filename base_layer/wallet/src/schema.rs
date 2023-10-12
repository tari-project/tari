// @generated automatically by Diesel CLI.

diesel::table! {
    burnt_proofs (id) {
        id -> Integer,
        reciprocal_claim_public_key -> Text,
        payload -> Text,
        burned_at -> Timestamp,
    }
}

diesel::table! {
    client_key_values (key) {
        key -> Text,
        value -> Text,
    }
}

diesel::table! {
    completed_transactions (tx_id) {
        tx_id -> BigInt,
        source_address -> Binary,
        destination_address -> Binary,
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
        mined_timestamp -> Nullable<Timestamp>,
        transaction_signature_nonce -> Binary,
        transaction_signature_key -> Binary,
    }
}

diesel::table! {
    inbound_transactions (tx_id) {
        tx_id -> BigInt,
        source_address -> Binary,
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

diesel::table! {
    known_one_sided_payment_scripts (script_hash) {
        script_hash -> Binary,
        private_key -> Text,
        script -> Binary,
        input -> Binary,
        script_lock_height -> BigInt,
    }
}

diesel::table! {
    outbound_transactions (tx_id) {
        tx_id -> BigInt,
        destination_address -> Binary,
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

diesel::table! {
    outputs (id) {
        id -> Integer,
        commitment -> Binary,
        rangeproof -> Nullable<Binary>,
        spending_key -> Text,
        value -> BigInt,
        output_type -> Integer,
        maturity -> BigInt,
        status -> Integer,
        hash -> Binary,
        script -> Binary,
        input_data -> Binary,
        script_private_key -> Text,
        script_lock_height -> BigInt,
        sender_offset_public_key -> Binary,
        metadata_signature_ephemeral_commitment -> Binary,
        metadata_signature_ephemeral_pubkey -> Binary,
        metadata_signature_u_a -> Binary,
        metadata_signature_u_x -> Binary,
        metadata_signature_u_y -> Binary,
        mined_height -> Nullable<BigInt>,
        mined_in_block -> Nullable<Binary>,
        marked_deleted_at_height -> Nullable<BigInt>,
        marked_deleted_in_block -> Nullable<Binary>,
        received_in_tx_id -> Nullable<BigInt>,
        spent_in_tx_id -> Nullable<BigInt>,
        coinbase_block_height -> Nullable<BigInt>,
        coinbase_extra -> Nullable<Binary>,
        features_json -> Text,
        spending_priority -> Integer,
        covenant -> Binary,
        mined_timestamp -> Nullable<Timestamp>,
        encrypted_data -> Binary,
        minimum_value_promise -> BigInt,
        source -> Integer,
        last_validation_timestamp -> Nullable<Timestamp>,
    }
}

diesel::table! {
    scanned_blocks (header_hash) {
        header_hash -> Binary,
        height -> BigInt,
        num_outputs -> Nullable<BigInt>,
        amount -> Nullable<BigInt>,
        timestamp -> Timestamp,
    }
}

diesel::table! {
    wallet_settings (key) {
        key -> Text,
        value -> Text,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    burnt_proofs,
    client_key_values,
    completed_transactions,
    inbound_transactions,
    known_one_sided_payment_scripts,
    outbound_transactions,
    outputs,
    scanned_blocks,
    wallet_settings,
);
