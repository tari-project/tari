create table metadata
(
    id               INT       NOT NULL PRIMARY KEY check (id = 0),
    chain_height     BIGINT   null,
    best_block       TEXT      null,
    accumulated_work BIGINT   null,
    pruning_horizon  BIGINT   NOT NULL,
    created_at       TIMESTAMP NOT NULL default CURRENT_TIMESTAMP,
    updated_at       TIMESTAMP NOT NULL default current_timestamp
);

-- There should always be one row
insert into metadata(id, chain_height, best_block, accumulated_work, pruning_horizon)
values (0, null, null, null, 0);

select diesel_manage_updated_at('metadata');
