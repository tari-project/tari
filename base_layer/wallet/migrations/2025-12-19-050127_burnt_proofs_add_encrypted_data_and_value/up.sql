
alter table burn_proofs
    add column encrypted_data blob null;

alter table burn_proofs
    add column value bigint null;

ALTER TABLE burn_proofs
    ADD COLUMN kernel_excess BLOB NULL;

ALTER TABLE burn_proofs
    ADD COLUMN kernel_excess_sig BLOB NULL;
