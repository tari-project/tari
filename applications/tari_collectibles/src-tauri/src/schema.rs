table! {
    accounts (id) {
        id -> Binary,
        asset_public_key -> Binary,
        name -> Nullable<Text>,
        description -> Nullable<Text>,
        image -> Nullable<Text>,
    }
}
