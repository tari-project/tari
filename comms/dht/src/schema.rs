// Copyright 2022 The Taiji Project
// SPDX-License-Identifier: BSD-3-Clause

table! {
    dedup_cache (id) {
        id -> Integer,
        body_hash -> Text,
        sender_public_key -> Text,
        number_of_hits -> Integer,
        stored_at -> Timestamp,
        last_hit_at -> Timestamp,
    }
}

table! {
    dht_metadata (id) {
        id -> Integer,
        key -> Text,
        value -> Binary,
    }
}

table! {
    stored_messages (id) {
        id -> Integer,
        version -> Integer,
        origin_pubkey -> Nullable<Text>,
        message_type -> Integer,
        destination_pubkey -> Nullable<Text>,
        destination_node_id -> Nullable<Text>,
        header -> Binary,
        body -> Binary,
        is_encrypted -> Bool,
        priority -> Integer,
        stored_at -> Timestamp,
        body_hash -> Text,
    }
}

allow_tables_to_appear_in_same_query!(dedup_cache, dht_metadata, stored_messages,);
