create table kernels (
    hash text not null primary key,
    features jsonb not null,
    fee bigint not null,
    lock_height bigint not null,
    meta_info text null,
    linked_kernel text null,
    excess text not null,
    excess_sig_nonce bytea not null,
    excess_sig_sig bytea not null,
    created_at timestamp not null default current_timestamp,
);

create index_kernels_hash on kernels(hash);
cluster kernels using index_kernels_hash;

select diesel_manage_updated_at('kernels');
