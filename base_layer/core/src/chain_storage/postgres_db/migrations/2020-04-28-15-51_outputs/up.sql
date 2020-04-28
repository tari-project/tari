create table  if not exists  outputs(
    id UUID PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
    created_in_block TEXT NOT NULL REFERENCES block_headers(hash),
    tx_output TEXT NOT NULL REFERENCES tx_outputs(hash)
);

create index index_outputs_id on outputs(id);
create index index_outputs_created_in_block on outputs(created_in_block);
create index index_outputs_tx_output on outputs(tx_output);
