PRAGMA foreign_keys=OFF;
DROP TABLE IF EXISTS key_manager_states;

CREATE TABLE key_manager_states (
                                    id                INTEGER PRIMARY KEY   NOT NULL,
                                    branch_seed       TEXT UNIQUE           NOT NULL,
                                    primary_key_index BLOB               NOT NULL,
                                    timestamp         DATETIME              NOT NULL
);

PRAGMA foreign_keys=ON;
