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
    send_count                  INTEGER DEFAULT 0  NOT NULL,
    last_send_timestamp         DATETIME,
    confirmations               BIGINT,
    mined_height                BIGINT,
    mined_in_block              BLOB,
    mined_timestamp             DATETIME,
    transaction_signature_nonce BLOB    DEFAULT 0  NOT NULL,
    transaction_signature_key   BLOB    DEFAULT 0  NOT NULL,
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
    direct_send_success INTEGER DEFAULT 0  NOT NULL,
    send_count          INTEGER DEFAULT 0  NOT NULL,
    last_send_timestamp DATETIME,
)

CREATE TABLE key_manager_states (
    id                INTEGER PRIMARY KEY NOT NULL,
    branch_seed       TEXT UNIQUE         NOT NULL,
    primary_key_index BLOB                NOT NULL,
    timestamp         DATETIME            NOT NULL,
)

CREATE TABLE key_manager_states_old (
    id                BIGINT PRIMARY KEY NOT NULL,
    seed              BLOB               NOT NULL,
    branch_seed       TEXT               NOT NULL,
    primary_key_index BIGINT             NOT NULL,
    timestamp         DATETIME           NOT NULL,
)

CREATE TABLE known_one_sided_payment_scripts (
    script_hash        BLOB PRIMARY KEY NOT NULL,
    private_key        BLOB             NOT NULL,
    script             BLOB             NOT NULL,
    input              BLOB             NOT NULL,
    script_lock_height UNSIGNED BIGINT  NOT NULL DEFAULT 0
)

CREATE TABLE outbound_transactions (
    tx_id                  BIGINT PRIMARY KEY NOT NULL,
    destination_public_key BLOB               NOT NULL,
    amount                 BIGINT             NOT NULL,
    fee                    BIGINT             NOT NULL,
    sender_protocol        TEXT               NOT NULL,
    message                TEXT               NOT NULL,
    timestamp              DATETIME           NOT NULL,
    cancelled              INTEGER DEFAULT 0  NOT NULL,
    direct_send_success    INTEGER DEFAULT 0  NOT NULL,
    send_count             INTEGER DEFAULT 0  NOT NULL,
    last_send_timestamp    DATETIME           NOT NULL,
)

CREATE TABLE outputs (
    id                         INTEGER PRIMARY KEY NOT NULL,
    commitment                 BLOB,
    spending_key               BLOB                NOT NULL,
    value                      BIGINT              NOT NULL,
    output_type                INTEGER             NOT NULL,
    maturity                   BIGINT              NOT NULL,
    status                     INTEGER             NOT NULL,
    hash                       BLOB,
    script                     BLOB                NOT NULL,
    input_data                 BLOB                NOT NULL,
    script_private_key         BLOB                NOT NULL,
    script_lock_height         BIGINT              NOT NULL,
    sender_offset_public_key   BLOB                NOT NULL,
    metadata_signature_nonce   BLOB                NOT NULL,
    metadata_signature_y_key   BLOB                NOT NULL,
    metadata_signature_v_key   BLOB                NOT NULL,
    mined_height               BIGINT,
    mined_in_block             BLOB,
    mined_mmr_position         BLOB,
    marked_deleted_at_height   BIGINT,
    marked_deleted_in_block    BLOB,
    received_in_tx_id          BIGINT,
    spent_in_tx_id             BIGINT,
    coinbase_block_height      BIGINT,
    metadata                   BLOB,
    features_parent_public_key BLOB,
    features_unique_id         BLOB,
    features_json              TEXT                NOT NULL,
    spending_priority          INTEGER             NOT NULL,
    covenant                   BLOB                NOT NULL,
    mined_timestamp            DATETIME,
    encrypted_value            BLOB                NOT NULL,
    contract_id                BLOB,
    minimum_value_precision    BIGINT              NOT NULL,
)

CREATE TABLE scanned_blocks (
    header_hash BLOB PRIMARY KEY NOT NULL,
    height      BIGINT           NOT NULL,
    num_outputs BIGINT,
    amount      BIGINT,
    timestamp   DATETIME         NOT NULL,
)

CREATE TABLE wallet_settings (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT             NOT NULL,
)