-- Your SQL goes here
create table wallets (
   id blob not null primary key,
   name text,
   cipher_seed blob not null unique
);