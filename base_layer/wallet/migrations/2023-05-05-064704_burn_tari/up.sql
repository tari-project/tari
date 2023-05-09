CREATE TABLE burnt_proofs
(
    id                          INTEGER PRIMARY KEY NOT NULL,
    reciprocal_claim_public_key TEXT                NOT NULL,
    payload                     TEXT                NOT NULL,
    burned_at                   DATETIME            NOT NULL
);
