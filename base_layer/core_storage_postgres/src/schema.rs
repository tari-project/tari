table! {
    block_headers (hash) {
        hash -> Text,
        height -> Int8,
        version -> Int4,
        prev_hash -> Text,
        timestamp -> Int8,
        output_mmr -> Text,
        range_proof_mmr -> Text,
        kernel_mmr -> Text,
        total_kernel_offset -> Text,
        nonce -> Int8,
        proof_of_work -> Jsonb,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

table! {
    merkle_checkpoints (id) {
        id -> Int8,
        mmr_tree -> Text,
        is_current -> Bool,
        nodes_added -> Array<Text>,
        nodes_deleted -> Bytea,
        rank -> Int8,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

table! {
    metadata (id) {
        id -> Int4,
        chain_height -> Nullable<Int8>,
        best_block -> Nullable<Text>,
        accumulated_work -> Nullable<Int8>,
        pruning_horizon -> Int8,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

table! {
    orphan_blocks (hash) {
        hash -> Text,
        header -> Jsonb,
        body -> Jsonb,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

table! {
    transaction_kernels (hash) {
        hash -> Text,
        features -> Int4,
        fee -> Numeric,
        lock_height -> Numeric,
        meta_info -> Nullable<Text>,
        linked_kernal -> Nullable<Text>,
        excess -> Text,
        excess_sig -> Bytea,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

table! {
    unspent_outputs (hash) {
        hash -> Text,
        features_flags -> Int4,
        features_maturity -> Numeric,
        commitment -> Text,
        proof -> Bytea,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

allow_tables_to_appear_in_same_query!(
    block_headers,
    merkle_checkpoints,
    metadata,
    orphan_blocks,
    transaction_kernels,
    unspent_outputs,
);
