table! {
    contacts (pub_key) {
        pub_key -> Text,
        screen_name -> Text,
        address -> Text,
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
    pending_transaction_outputs (tx_id) {
        tx_id -> BigInt,
        timestamp -> Timestamp,
    }
}

table! {
    received_messages (id) {
        id -> Binary,
        source_pub_key -> Text,
        dest_pub_key -> Text,
        message -> Text,
        timestamp -> Timestamp,
    }
}

table! {
    sent_messages (id) {
        id -> Text,
        source_pub_key -> Text,
        dest_pub_key -> Text,
        message -> Text,
        timestamp -> Timestamp,
        acknowledged -> Integer,
        is_read -> Integer,
    }
}

table! {
    settings (pub_key) {
        pub_key -> Text,
        screen_name -> Text,
    }
}

joinable!(outputs -> pending_transaction_outputs (tx_id));
joinable!(sent_messages -> contacts (dest_pub_key));

allow_tables_to_appear_in_same_query!(
    contacts,
    outputs,
    pending_transaction_outputs,
    received_messages,
    sent_messages,
    settings,
);
