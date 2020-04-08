create table tx_outputs(
    hash TEXT NOT NULL PRIMARY KEY,
    features_flags SMALLINT NOT NULL,
    features_maturity BIGINT NOT NULL,
    commitment TEXT NOT NULL,
    proof BYTEA null,
    spent BIGINT NOT NULL default 0,
    created_at TIMESTAMP NOT NULL default current_timestamp,
);

create index index_tx_outputs_hash on tx_outputs(hash);
create index index_tx_outputs_spent on tx_outputs(boolean);
cluster tx_outputs using index_tx_outputs_hash;

select diesel_manage_updated_at('tx_outputs');
