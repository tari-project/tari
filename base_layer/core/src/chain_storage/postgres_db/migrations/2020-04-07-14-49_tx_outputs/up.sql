create table  if not exists  tx_outputs(
    hash TEXT NOT NULL PRIMARY KEY,
    features_flags SMALLINT NOT NULL,
    features_maturity BIGINT NOT NULL,
    commitment TEXT NOT NULL,
    proof BYTEA NULL,
);

create index index_tx_outputs_hash on tx_outputs(hash);
create index index_tx_outputs_created_in_block on tx_outputs(created_in_block);
create index index_tx_outputs_spent on tx_outputs(spent);
cluster tx_outputs using index_tx_outputs_hash;
