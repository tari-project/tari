create table accounts (
   id blob not null primary key,
   asset_public_key blob not null unique,
   name text,
   description text,
   image text,
   committee_length integer not null,
   committee_pub_keys blob not null
);