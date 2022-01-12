create table key_indices (
   id blob not null primary key,
   branch_seed text not null unique,
   last_index BigInt not null
);