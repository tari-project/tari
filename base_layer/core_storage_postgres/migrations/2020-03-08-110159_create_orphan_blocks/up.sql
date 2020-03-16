create table orphan_blocks (
    hash text not null primary key,
    header jsonb not null,
    body jsonb not null,
    created_at timestamp not null default current_timestamp,
    updated_at timestamp not null default current_timestamp
);
select diesel_manage_updated_at('orphan_blocks');