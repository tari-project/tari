table! {
    instructions (id) {
        id -> Integer,
        hash -> Binary,
        node_hash -> Binary,
        asset_id -> Binary,
        template_id -> Integer,
        method -> Text,
        args -> Binary,
    }
}

table! {
    locked_qc (id) {
        id -> Integer,
        message_type -> Integer,
        view_number -> BigInt,
        node_hash -> Binary,
        signature -> Nullable<Binary>,
    }
}

table! {
    nodes (hash) {
        hash -> Binary,
        parent -> Binary,
    }
}

table! {
    prepare_qc (id) {
        id -> Integer,
        message_type -> Integer,
        view_number -> BigInt,
        node_hash -> Binary,
        signature -> Nullable<Binary>,
    }
}

table! {
    state_key_values (id) {
        id -> Integer,
        schema_name -> Text,
        key -> Binary,
        value -> Binary,
    }
}

allow_tables_to_appear_in_same_query!(
    instructions,
    locked_qc,
    nodes,
    prepare_qc,
    state_key_values,
);
