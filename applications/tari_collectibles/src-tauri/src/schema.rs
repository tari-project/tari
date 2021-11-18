table! {
    accounts (id) {
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
    wallets (id) {
        id -> Binary,
        name -> Nullable<Text>,
        cipher_seed -> Binary,
    }
}
