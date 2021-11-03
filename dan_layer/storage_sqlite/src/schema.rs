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
    nodes (hash) {
        hash -> Binary,
        parent -> Binary,
    }
}

allow_tables_to_appear_in_same_query!(instructions, nodes,);
