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
    }
}

table! {
    outputs (spending_key) {
        spending_key -> Binary,
        value -> BigInt,
        flags -> Integer,
        maturity -> BigInt,
        spent -> Integer,
        to_be_received -> Integer,
        encumbered -> Integer,
        tx_id -> Nullable<BigInt>,
    }
}

table! {
    peers (public_key) {
        public_key -> Binary,
        peer -> Text,
    }
}

table! {
    pending_transaction_outputs (tx_id) {
        tx_id -> BigInt,
        timestamp -> Timestamp,
    }
}

joinable!(outputs -> pending_transaction_outputs (tx_id));

allow_tables_to_appear_in_same_query!(
    completed_transactions,
    contacts,
    inbound_transactions,
    key_manager_states,
    outbound_transactions,
    outputs,
    peers,
    pending_transaction_outputs,
);
