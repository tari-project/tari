create table block_headers (
    hash TEXT NOT NULL PRIMARY KEY,
    height BIGINT NOT NULL,
    version INT NOT NULL,
    prev_hash TEXT NOT NULL,
    TIMESTAMP BIGINT NOT NULL,
    output_mmr TEXT NOT NULL,
    range_proof_mmr TEXT NOT NULL,
    kernel_mmr TEXT NOT NULL,
    total_kernel_offset TEXT NOT NULL,
    nonce BIGINT NOT NULL,
    proof_of_work_display jsonb NOT NULL,
    orphan BOOLEAN NOT NULL DEFAULT false,
);

create index index_block_headers_hash on block_headers(hash);
create index index_block_headers_height on block_headers(height);
cluster block_headers using index_block_headers_height;

select diesel_manage_updated_at('block_headers');
