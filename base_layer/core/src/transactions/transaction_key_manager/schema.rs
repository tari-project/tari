// @generated automatically by Diesel CLI.

diesel::table! {
    imported_keys (id) {
        id -> Integer,
        private_key -> Binary,
        public_key -> Text,
        timestamp -> Timestamp,
    }
}

diesel::table! {
    key_manager_states (id) {
        id -> Integer,
        branch_seed -> Text,
        primary_key_index -> Binary,
        timestamp -> Timestamp,
    }
}

diesel::allow_tables_to_appear_in_same_query!(imported_keys, key_manager_states,);
