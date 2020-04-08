create table inputs (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
    block_hash TEXT NOT NULL,
    tx_output TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL default current_timestamp,
);

create index index_inputs_id on inputs(id);
create index index_inputs_block_hash on inputs(hash);
create index index_inputs_output_hash on inputs(height);
cluster inputs using inputs_id;

select diesel_manage_updated_at('inputs');
