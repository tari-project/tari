-- Your SQL goes here
create table nodes (
    id integer primary key autoincrement not null,
    hash blob not null unique,
    parent blob not null,
    height integer not null,
    is_committed boolean not null DEFAULT FALSE
);

create table instructions (
    id integer primary  key autoincrement not null,
    hash blob not null,
    node_id integer not null ,
    template_id int not null,
    method text not null,
    args blob not null,
    foreign key (node_id) references nodes(id)
);


create table locked_qc (
    id integer primary key not null, -- should always be 1 row
    message_type integer not null,
    view_number bigint not null,
    node_hash blob not null,
    signature blob null
);

create table prepare_qc (
                           id integer primary key not null,
                           message_type integer not null,
                           view_number bigint not null,
                           node_hash blob not null,
                           signature blob null
);


create table state_key_values (
    id integer primary key autoincrement  not null,
    schema_name text not null,
    key blob not null,
    value blob not null
);