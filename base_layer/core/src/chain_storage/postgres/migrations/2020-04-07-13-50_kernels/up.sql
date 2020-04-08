create table kernels (
    hash TEXT NOT NULL PRIMARY KEY,
    features jsonb NOT NULL,
    fee BIGINT NOT NULL,
    lock_height BIGINT NOT NULL,
    meta_info TEXT null,
    linked_kernel TEXT null,
    excess TEXT NOT NULL,
    excess_sig_nonce BYTEA NOT NULL,
    excess_sig_sig BYTEA NOT NULL,
    block_hash TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL default current_timestamp,
);

create index index_kernels_hash on kernels(hash);
create index index_kernels_block_hash on kernels(block_hash);
cluster kernels using index_kernels_hash;

select diesel_manage_updated_at('kernels');
