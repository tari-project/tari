// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use diesel::{allow_tables_to_appear_in_same_query, joinable, table};

table! {
    node_identity (public_key) {
        public_key -> Text,
        node_id -> Text,
        features -> Integer,
    }
}

table! {
    peers (peer_id) {
        peer_id -> BigInt,
        public_key -> Text,
        node_id -> Text,
        flags -> Integer,
        banned_until -> Nullable<Timestamp>,
        banned_reason -> Text,
        features -> Integer,
        supported_protocols -> Text,
        added_at -> Timestamp,
        user_agent -> Text,
        metadata -> Nullable<Binary>,
        deleted_at -> Nullable<Timestamp>,
    }
}

table! {
    multi_addresses (address_id) {
        address_id -> Nullable<Integer>,
        peer_id -> BigInt,
        address -> Text,
        is_external -> Bool,
        last_seen -> Nullable<Timestamp>,
        connection_attempts -> Integer,
        avg_initial_dial_time -> Nullable<BigInt>,
        initial_dial_time_sample_count -> Integer,
        avg_latency -> Nullable<BigInt>,
        latency_sample_count -> Integer,
        last_attempted -> Nullable<Timestamp>,
        last_failed_reason -> Nullable<Text>,
        quality_score -> Nullable<Integer>,
        source -> Text,
    }
}

table! {
    db_metadata (key) {
        key -> Text,
        value -> Text,
    }
}

allow_tables_to_appear_in_same_query!(peers, multi_addresses);
joinable!(multi_addresses -> peers (peer_id));
