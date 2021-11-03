-- Your SQL goes here
create table nodes (
    hash blob not null primary key,
    parent blob not null
);

create table instructions (
    id integer primary  key autoincrement not null,
    hash blob not null,
    node_hash blob not null,
    asset_id blob not null,
    template_id int not null,
    method text not null,
    args blob not null
);