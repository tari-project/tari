create table transaction_kernels (
    hash text not null primary key,
    features integer not null,
    fee bigint not null,
    lock_height bigint not null,
    meta_info text null,
    linked_kernel text null,
    excess text not null,
    excess_sig_nonce bytea not null,
    excess_sig_sig bytea not null,
    created_at timestamp not null default current_timestamp,
    updated_at timestamp not null default current_timestamp
);

-- select diesel_manage_updated_at('transaction_kernels');