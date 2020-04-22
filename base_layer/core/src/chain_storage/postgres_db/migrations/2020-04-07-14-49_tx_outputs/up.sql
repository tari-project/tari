create table  if not exists  tx_outputs(
    hash TEXT NOT NULL PRIMARY KEY,
    features_flags SMALLINT NOT NULL,
    features_maturity BIGINT NOT NULL,
    commitment TEXT NOT NULL,
    proof BYTEA NULL,
    tx_output TEXT NULL REFERENCES block_headers(hash),
    spent TEXT NULL REFERENCES block_headers(hash)
);

create index index_tx_outputs_hash on tx_outputs(hash);
create index index_tx_outputs_tx_output on tx_outputs(tx_output);
create index index_tx_outputs_spent on tx_outputs(spent);
cluster tx_outputs using index_tx_outputs_hash;