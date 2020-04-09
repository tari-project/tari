create table kernels (
    hash TEXT NOT NULL PRIMARY KEY,
    features SMALLINT NOT NULL,
    fee BIGINT NOT NULL,
    lock_height BIGINT NOT NULL,
    meta_info TEXT NULL,
    linked_kernel TEXT NULL,
    excess TEXT NOT NULL,
    excess_sig_nonce BYTEA NOT NULL,
    excess_sig_sig BYTEA NOT NULL,
    block_hash TEXT NOT NULL REFERENCES block_headers(hash),
    created_at TIMESTAMP NOT NULL DEFAULT current_timestamp,
);

create index index_kernels_hash on kernels(hash);
create index index_kernels_block_hash on kernels(block_hash);
cluster kernels using index_kernels_hash;

select diesel_manage_updated_at('kernels');
