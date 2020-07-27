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
        cancelled -> Integer,
        direction -> Nullable<Integer>,
    }
}

table! {
    contacts (public_key) {
        public_key -> Binary,
        alias -> Text,
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
    }
}

table! {
    key_manager_states (id) {
        id -> Nullable<BigInt>,
        master_seed -> Binary,
        branch_seed -> Text,
        primary_key_index -> BigInt,
        timestamp -> Timestamp,
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
        tx_id -> Nullable<BigInt>,
        hash -> Nullable<Binary>,
    }
}

table! {
    pending_transaction_outputs (tx_id) {
        tx_id -> BigInt,
        short_term -> Integer,
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
    completed_transactions,
    contacts,
    inbound_transactions,
    key_manager_states,
    outbound_transactions,
    outputs,
    pending_transaction_outputs,
    wallet_settings,
);
