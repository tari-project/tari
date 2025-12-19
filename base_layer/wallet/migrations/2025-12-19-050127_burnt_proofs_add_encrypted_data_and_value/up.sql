
alter table burn_proofs
    add column encrypted_data blob null;

alter table burn_proofs
    add column value bigint null;