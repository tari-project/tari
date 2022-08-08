CREATE TABLE client_key_values (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT             NOT NULL,
)

CREATE TABLE completed_transactions (
    tx_id                  BIGINT PRIMARY KEY NOT NULL,
    source_public_key      BLOB               NOT NULL,
    destination_public_key BLOB               NOT NULL,
    amount                 BIGINT             NOT NULL,
    fee                    BIGINT             NOT NULL, 
    transaction_protocol   TEXT               NOT NULL,
    status                 INTEGER            NOT NULL,
    message                TEXT               NOT NULL,
    timestamp              DATETIME           NOT NULL,
    
)