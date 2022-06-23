ALTER TABLE outputs
    ADD contract_id blob NULL;

CREATE INDEX outputs_contract_id_index ON outputs (contract_id);