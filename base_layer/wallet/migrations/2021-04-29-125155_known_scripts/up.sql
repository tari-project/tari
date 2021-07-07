 CREATE TABLE known_one_sided_payment_scripts (
    script_hash BLOB PRIMARY KEY NOT NULL,
    private_key BLOB NOT NULL,
    script BLOB NOT NULL,
    input BLOB  NOT NULL
 );
