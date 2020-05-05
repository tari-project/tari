create table  if not exists  spends(
    id UUID PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
    spent_in_block TEXT NOT NULL REFERENCES block_headers(hash),
    tx_output TEXT NOT NULL REFERENCES tx_outputs(hash)
);

create index index_spends_id on spends(id);
create index index_spends_spent_in_block on spends(spent_in_block);
create index index_spends_tx_output on spends(tx_output);
