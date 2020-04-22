create table if not exists metadata
(
    id               INT       NOT NULL PRIMARY KEY check (id = 0),
    chain_height     BIGINT   NULL,
    best_block       TEXT      NULL,
    accumulated_work BIGINT   NULL,
    pruning_horizon  BIGINT   NOT NULL,
    updated_at       TIMESTAMP NOT NULL DEFAULT current_timestamp
);

-- There should always be one row
insert into metadata(id, chain_height, best_block, accumulated_work, pruning_horizon)
values (0, NULL, NULL, NULL, 0);

select diesel_manage_updated_at('metadata');
