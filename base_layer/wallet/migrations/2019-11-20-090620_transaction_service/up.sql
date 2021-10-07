CREATE TABLE outbound_transactions (
    tx_id                  BIGINT PRIMARY KEY NOT NULL,
    destination_public_key BLOB               NOT NULL,
    amount                 BIGINT             NOT NULL,
    fee                    BIGINT             NOT NULL,
    sender_protocol        TEXT               NOT NULL,
    message                TEXT               NOT NULL,
    timestamp              DATETIME           NOT NULL
);

CREATE TABLE inbound_transactions (
    tx_id             BIGINT PRIMARY KEY NOT NULL,
    source_public_key BLOB               NOT NULL,
    amount            BIGINT             NOT NULL,
    receiver_protocol TEXT               NOT NULL,
    message           TEXT               NOT NULL,
    timestamp         DATETIME           NOT NULL
);

CREATE TABLE coinbase_transactions (
    tx_id      BIGINT PRIMARY KEY NOT NULL,
    amount     BIGINT             NOT NULL,
    commitment BLOB               NOT NULL,
    timestamp  DATETIME           NOT NULL
);

CREATE TABLE completed_transactions (
    tx_id                  BIGINT PRIMARY KEY NOT NULL,
    source_public_key      BLOB               NOT NULL,
    destination_public_key BLOB               NOT NULL,
    amount                 BIGINT             NOT NULL,
    fee                    BIGINT             NOT NULL,
    transaction_protocol   TEXT               NOT NULL,
    status                 INTEGER            NOT NULL,
    message                TEXT               NOT NULL,
    timestamp              DATETIME           NOT NULL
);
