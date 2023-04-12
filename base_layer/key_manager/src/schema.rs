// @generated automatically by Diesel CLI.

diesel::table! {
    key_manager_states (id) {
        id -> Integer,
        branch_seed -> Text,
        primary_key_index -> Binary,
        timestamp -> Timestamp,
    }
}
