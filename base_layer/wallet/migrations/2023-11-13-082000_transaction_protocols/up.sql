-- Any old 'inbound_transactions' will not be valid due to the change in 'receiver_protocol' to 'BLOB', so we drop and
-- recreate the table.

DROP TABLE inbound_transactions;
CREATE TABLE inbound_transactions
(
    tx_id               BIGINT PRIMARY KEY NOT NULL,
    source_address      BLOB               NOT NULL,
    amount              BIGINT             NOT NULL,
    receiver_protocol   BLOB               NOT NULL,
    message             TEXT               NOT NULL,
    timestamp           DATETIME           NOT NULL,
    cancelled           INTEGER            NOT NULL,
    direct_send_success INTEGER DEFAULT 0  NOT NULL,
    send_count          INTEGER DEFAULT 0  NOT NULL,
    last_send_timestamp DATETIME           NULL
);

-- Any old 'outbound_transactions' will not be valid due to the change in 'sender_protocol' to 'BLOB', so we drop and
-- recreate the table.

DROP TABLE outbound_transactions;
CREATE TABLE outbound_transactions
(
    tx_id               BIGINT PRIMARY KEY NOT NULL,
    destination_address BLOB               NOT NULL,
    amount              BIGINT             NOT NULL,
    fee                 BIGINT             NOT NULL,
    sender_protocol     BLOB               NOT NULL,
    message             TEXT               NOT NULL,
    timestamp           DATETIME           NOT NULL,
    cancelled           INTEGER DEFAULT 0  NOT NULL,
    direct_send_success INTEGER DEFAULT 0  NOT NULL,
    send_count          INTEGER DEFAULT 0  NOT NULL,
    last_send_timestamp DATETIME           NULL
);
