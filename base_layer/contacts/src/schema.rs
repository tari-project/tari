// @generated automatically by Diesel CLI.

diesel::table! {
    contacts (address) {
        address -> Binary,
        node_id -> Binary,
        alias -> Text,
        last_seen -> Nullable<Timestamp>,
        latency -> Nullable<Integer>,
        favourite -> Integer,
    }
}

diesel::table! {
    messages (message_id) {
        address -> Binary,
        message_id -> Binary,
        body -> Binary,
        metadata -> Binary,
        stored_at -> Timestamp,
        delivery_confirmation_at -> Timestamp,
        read_confirmation_at -> Timestamp,
        direction -> Integer,
    }
}
