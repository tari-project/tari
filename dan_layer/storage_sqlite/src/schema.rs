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

allow_tables_to_appear_in_same_query!(
    instructions,
    locked_qc,
    nodes,
);
