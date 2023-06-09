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
