create table assets (
                        id blob not null primary key,
                        asset_public_key blob not null unique,
                        name text,
                        description text,
                        image text,
                        committee_length integer not null,
                        committee_pub_keys blob not null
);

create table wallets (
   id blob not null primary key,
   name text,
   cipher_seed blob not null unique
);

create table asset_wallets (
    id blob not null primary key,
    asset_id blob not null references assets(id),
    wallet_id blob not null references wallets(id)
);

create table addresses (
    id blob not null primary key,
    asset_wallet_id blob not null references asset_wallets (id),
    name text,
    public_key blob not null,
    key_manager_path TEXT not null
    );

create table tip002_address (
    id blob not null primary key,
    address_id blob not null references addresses(id),
    balance bigint not null,
    at_height bigint
);

create table issued_assets (
    id blob not null primary key,
    wallet_id blob not null references wallets (id),
    public_key blob not null,
    key_manager_path text not null,
    is_draft boolean not null
);

