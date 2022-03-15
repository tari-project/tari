table! {
    instructions (id) {
        id -> Integer,
        hash -> Binary,
        node_id -> Integer,
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
    nodes (id) {
        id -> Integer,
        hash -> Binary,
        parent -> Binary,
        height -> Integer,
        is_committed -> Bool,
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
    state_keys (schema_name, key_name) {
        schema_name -> Text,
        key_name -> Binary,
        value -> Binary,
    }
}

table! {
    state_op_log (id) {
        id -> Integer,
        height -> BigInt,
        merkle_root -> Nullable<Binary>,
        operation -> Text,
        schema -> Text,
        key -> Binary,
        value -> Nullable<Binary>,
    }
}

table! {
    state_tree (id) {
        id -> Integer,
        version -> Integer,
        is_current -> Bool,
        data -> Binary,
    }
}

joinable!(instructions -> nodes (node_id));

allow_tables_to_appear_in_same_query!(
    instructions,
    locked_qc,
    nodes,
    prepare_qc,
    state_keys,
    state_op_log,
    state_tree,
);
