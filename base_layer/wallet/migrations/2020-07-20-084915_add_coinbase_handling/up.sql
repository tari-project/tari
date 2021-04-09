-- Rename the master_seed column to master_key
PRAGMA foreign_keys=off;
ALTER TABLE key_manager_states RENAME TO key_manager_states_old;

CREATE TABLE key_manager_states (
                                    id INTEGER PRIMARY KEY,
                                    master_key BLOB NOT NULL,
                                    branch_seed TEXT NOT NULL,
                                    primary_key_index INTEGER NOT NULL,
                                    timestamp DATETIME NOT NULL
);
INSERT INTO key_manager_states (id, master_key, branch_seed, primary_key_index, timestamp)
SELECT id, master_seed, branch_seed, primary_key_index, timestamp
    FROM key_manager_states_old;
DROP TABLE key_manager_states_old;

PRAGMA foreign_keys=on;
ALTER TABLE pending_transaction_outputs ADD COLUMN coinbase_block_height INTEGER NULL;

ALTER TABLE completed_transactions ADD COLUMN coinbase_block_height INTEGER NULL;
