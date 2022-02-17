create table state_op_log
(
    id          integer primary key autoincrement not null,
    height      bigint                            not null,
    merkle_root blob(32)                          null,
    operation   varchar(30)                       not null,
    schema      varchar(255)                      not null,
    key         blob                              not null,
    value       blob                              null
);

create index state_op_log_height_index on state_op_log (height);
create index state_op_log_merkle_root_index on state_op_log (merkle_root);
