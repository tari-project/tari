PRAGMA foreign_keys=OFF;
ALTER TABLE key_manager_states
    RENAME TO key_manager_states_old;

CREATE TABLE key_manager_states (
                                    id                INTEGER PRIMARY KEY   NOT NULL,
                                    branch_seed       TEXT UNIQUE           NOT NULL,
                                    primary_key_index BLOB               NOT NULL,
                                    timestamp         DATETIME              NOT NULL
);

PRAGMA foreign_keys=ON;
