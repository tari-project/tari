-- Add the constraint back to the table
ALTER TABLE multi_addresses ADD CONSTRAINT unique_address UNIQUE (address);