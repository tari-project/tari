CREATE TABLE client_key_values (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT             NOT NULL,
)

CREATE TABLE completed_transactions (
    tx_id                       BIGINT PRIMARY KEY NOT NULL,
    source_public_key           BLOB               NOT NULL,
    destination_public_key      BLOB               NOT NULL,
    amount                      BIGINT             NOT NULL,
    fee                         BIGINT             NOT NULL, 
    transaction_protocol        TEXT               NOT NULL,
    status                      INTEGER            NOT NULL,
    message                     TEXT               NOT NULL,
    timestamp                   DATETIME           NOT NULL,
    cancelled                   INTEGER,
    direction                   INTEGER,
    coinbase_block_height       BIGINT,
    send_count                  INTEGER default 0  NOT NULL,
    last_send_timestamp         DATETIME,
    confirmations               BIGINT,
    mined_height                BIGINT,
    mined_in_block              BLOB,
    mined_timestamp             DATETIME,
    transaction_signature_nonce BLOB    default 0  NOT NULL,
    transaction_signature_key   BLOB    default 0  NOT NULL,
)

CREATE TABLE contacts (
    public_key  BLOB PRIMARY KEY NOT NULL UNIQUE,
    node_id     BLOB             NOT NULL UNIQUE,
    alias       TEXT             NOT NULL,
    last_seen   DATETIME,
    latency     INTEGER,
)

CREATE TABLE inbound_transactions (
    tx_id               BIGINT PRIMARY KEY NOT NULL,
    source_public_key   BLOB               NOT NULL,
    amount              BIGINT             NOT NULL,
    receiver_protocol   TEXT               NOT NULL,
    message             TEXT               NOT NULL,
    timestamp           DATETIME           NOT NULL,
    cancelled           INTEGER            NOT NULL,
    direct_send_success INTEGER            NOT NULL,
    send_count          INTEGER            NOT NULL,
    last_send_timestamp TIMESTAMP,
)

CREATE TABLE key_manager_states (
    id                INTEGER PRIMARY KEY NOT NULL,
    branch_seed       TEXT                NOT NULL,
    primary_key_index BLOB                NOT NULL,
    timestamp         DATETIME            NOT NULL,
)