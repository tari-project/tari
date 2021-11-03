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

create table locked_qc (
    id integer primary key not null, -- should always be 1 row
    message_type integer not null,
    view_number bigint not null,
    node_hash blob not null,
    signature blob null
);

create table prepare_qcs (
                           id integer primary key autoincrement not null,
                           message_type integer not null,
                           view_number bigint not null,
                           node_hash blob not null,
                           signature blob null
)
