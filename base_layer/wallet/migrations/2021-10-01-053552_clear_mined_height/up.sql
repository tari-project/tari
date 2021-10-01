-- mined_height and mined_in_block should always be set together, since mined_in_block is NULL we set mined_height to NULL
-- so that the transactions can be revalidated.
UPDATE completed_transactions
SET mined_height = NULL
WHERE mined_height IS NOT NULL AND mined_in_block IS NULL;