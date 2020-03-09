create table block_headers (
    hash text not null primary key,
    height bigint not null,
    version int not null,
    prev_hash text not null,
    timestamp bigint not null,
    output_mmr text not null,
    range_proof_mmr text not null,
    kernel_mmr text not null,
    total_kernel_offset numeric not null,
    nonce numeric not null,
    proof_of_work jsonb not null,
    created_at timestamp not null default current_timestamp,
    updated_at timestamp not null default current_timestamp
);

create index index_block_headers_height on block_headers(height);
cluster block_headers using index_block_headers_height;

select diesel_manage_updated_at('block_headers');