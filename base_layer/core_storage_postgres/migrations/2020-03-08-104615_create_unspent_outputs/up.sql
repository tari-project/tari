create table unspent_outputs(
    hash text not null primary key,
    features_flags int not null,
    features_maturity bigint not null,
    commitment text not null,
    proof bytea not null,
    created_at timestamp not null default current_timestamp,
    updated_at timestamp not null default current_timestamp
);

-- select diesel_manage_updated_at('unspent_outputs');