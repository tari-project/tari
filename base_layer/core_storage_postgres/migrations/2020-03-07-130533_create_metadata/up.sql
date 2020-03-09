-- There should always only be one row in this table. Diesel requires a primary key though
-- so we'll add a dummy one, and make sure it's always 0
create table metadata
(
    id               int       not null primary key check (id = 0),
    chain_height     bigint   null,
    best_block       text      null,
    accumulated_work bigint   null,
    pruning_horizon  bigint   not null,
    created_at       timestamp not null default CURRENT_TIMESTAMP,
    updated_at       timestamp not null default current_timestamp
);

-- There should always be one row
insert into metadata(id, chain_height, best_block, accumulated_work, pruning_horizon)
values (0, null, null, null, 0);

select diesel_manage_updated_at('metadata');