table! {
    addresses (id) {
        id -> Binary,
        asset_wallet_id -> Binary,
        name -> Nullable<Text>,
        public_key -> Binary,
        key_manager_path -> Text,
    }
}

table! {
    asset_wallets (id) {
        id -> Binary,
        asset_id -> Binary,
        wallet_id -> Binary,
    }
}

table! {
    assets (id) {
        id -> Binary,
        asset_public_key -> Binary,
        name -> Nullable<Text>,
        description -> Nullable<Text>,
        image -> Nullable<Text>,
        committee_length -> Integer,
        committee_pub_keys -> Binary,
    }
}

table! {
    issued_assets (id) {
        id -> Binary,
        wallet_id -> Binary,
        public_key -> Binary,
        key_manager_path -> Text,
        is_draft -> Bool,
    }
}

table! {
    key_indices (id) {
        id -> Binary,
        branch_seed -> Text,
        last_index -> BigInt,
    }
}

table! {
    tip002_address (id) {
        id -> Binary,
        address_id -> Binary,
        balance -> BigInt,
        at_height -> Nullable<BigInt>,
    }
}

table! {
    tip721_tokens (id) {
        id -> Binary,
        address_id -> Binary,
        token_id -> Binary,
        is_deleted -> Bool,
        token -> Text,
    }
}

table! {
    wallets (id) {
        id -> Binary,
        name -> Nullable<Text>,
        cipher_seed -> Binary,
    }
}

joinable!(addresses -> asset_wallets (asset_wallet_id));
joinable!(asset_wallets -> assets (asset_id));
joinable!(asset_wallets -> wallets (wallet_id));
joinable!(issued_assets -> wallets (wallet_id));
joinable!(tip002_address -> addresses (address_id));
joinable!(tip721_tokens -> addresses (address_id));

allow_tables_to_appear_in_same_query!(
    addresses,
    asset_wallets,
    assets,
    issued_assets,
    key_indices,
    tip002_address,
    tip721_tokens,
    wallets,
);
