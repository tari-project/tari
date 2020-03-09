create table merkle_checkpoints (
    id bigserial primary key ,
    mmr_tree text not null,
    is_current boolean not null,
    nodes_added text[] not null,
    nodes_deleted bytea not null,
    rank bigint not null,
    created_at timestamp not null default current_timestamp,
    updated_at timestamp not null default current_timestamp
);

select diesel_manage_updated_at('merkle_checkpoints');
create index index_merkle_checkpoints_mmr_tree on merkle_checkpoints (mmr_tree);
create unique index index_merkle_checkpoints_mmr_tree_rank on merkle_checkpoints(rank, mmr_tree);