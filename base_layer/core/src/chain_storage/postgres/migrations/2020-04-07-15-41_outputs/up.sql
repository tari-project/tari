create table outputs (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
    block_hash TEXT NOT NULL,
    tx_output TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT current_timestamp,
);

create index index_outputs_id on outputs(id);
create index index_outputs_block_hash on outputs(hash);
create index index_outputs_output_hash on outputs(height);
cluster outputs using inputs_id;

select diesel_manage_updated_at('outputs');
