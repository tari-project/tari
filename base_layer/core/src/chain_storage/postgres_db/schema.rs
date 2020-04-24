table! {
    block_headers (hash) {
        hash -> Text,
        height -> Int8,
        version -> Int4,
        prev_hash -> Text,
        time_stamp -> Int8,
        output_mmr -> Text,
        range_proof_mmr -> Text,
        kernel_mmr -> Text,
        total_kernel_offset -> Text,
        nonce -> Int8,
        proof_of_work -> Jsonb,
        orphan -> Bool,
    }
}

table! {
    kernels (hash) {
        hash -> Text,
        features -> Int2,
        fee -> Int8,
        lock_height -> Int8,
        meta_info -> Nullable<Text>,
        linked_kernel -> Nullable<Text>,
        excess -> Text,
        excess_sig_nonce -> Bytea,
        excess_sig_sig -> Bytea,
        block_hash -> Text,
        created_at -> Timestamp,
    }
}

table! {
    metadata (id) {
        id -> Int4,
        chain_height -> Nullable<Int8>,
        best_block -> Nullable<Text>,
        accumulated_work -> Nullable<Int8>,
        pruning_horizon -> Int8,
        updated_at -> Timestamp,
    }
}

table! {
    tx_outputs (hash) {
        hash -> Text,
        features_flags -> Int2,
        features_maturity -> Int8,
        commitment -> Text,
        proof -> Nullable<Bytea>,
        created_in_block -> Text,
        spent -> Nullable<Text>,
    }
}

joinable!(kernels -> block_headers (block_hash));

allow_tables_to_appear_in_same_query!(
    block_headers,
    kernels,
    metadata,
    tx_outputs,
);
