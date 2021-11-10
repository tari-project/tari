create table accounts (
   id blob not null primary key,
   asset_public_key blob not null unique
);