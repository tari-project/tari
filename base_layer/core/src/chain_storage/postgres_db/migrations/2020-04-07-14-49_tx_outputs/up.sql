create table tx_outputs(
    hash TEXT NOT NULL PRIMARY KEY,
    features_flags SMALLINT NOT NULL,
    features_maturity BIGINT NOT NULL,
    commitment TEXT NOT NULL,
    proof BYTEA NULL,
    input TEXT NOT NULL REFERENCES block_headers(hash),
    spent TEXT NULL REFERENCES block_headers(hash)
);

create index index_tx_outputs_hash on tx_outputs(hash);
create index index_tx_outputs_input on tx_outputs(input);
create index index_tx_outputs_spent on tx_outputs(spent);
cluster tx_outputs using index_tx_outputs_hash;

select diesel_manage_updated_at('tx_outputs');
