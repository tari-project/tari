// Copyright 2022 The Tari Project
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

allow_tables_to_appear_in_same_query!(dedup_cache, dht_metadata,);
