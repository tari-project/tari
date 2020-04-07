create table tx_outputs(
    hash text not null primary key,
    features_flags smallint not null,
    features_maturity bigint not null,
    commitment text not null,
    proof bytea not null,
    spent boolean not null default false,
    created_at timestamp not null default current_timestamp,
);

create index index_tx_outputs_hash on tx_outputs(hash);
create index index_tx_outputs_spent on tx_outputs(boolean);
cluster tx_outputs using index_tx_outputs_hash;

select diesel_manage_updated_at('tx_outputs');
