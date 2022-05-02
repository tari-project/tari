
-- contract template registration
insert into templates (source_url, source_type, source_hash, version_info, execution_engine_requirements)
values (
    'http://github.com/tari-templates/nft-project', -- source_url
    'source', -- source_type
    X'04e54b3dbb971c87f52f6bb8e2166adc9eea8a63fa8942171731b438fe2bc0f4', -- source_hash
    '1.0.0', -- version_info
    '>=0.6.0 <0.6.4;' -- execution_engine_requirements
);

-- contract registration
insert into contracts (id)
values (
    X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539'
);

-- the VN node sees on the blockchain the contract definition transaction
insert into contract_definitions (
    contract_id, name, description, asset_issuer_key, template_id, initialization_arguments, runtime_specification, stake, collateral
)
values (
    X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539', -- contract_id
    'Cool NFT', -- name
    'This is a cool NFT contract', -- description
    X'bec7f50a7307aff31eef64789bcd50e996e4b16b9f974cabef4800add830392f', -- asset_issuer_key
    1, -- template_id, references the "template" table
    X'00000000000000000000000000000001', -- initialization_arguments
    '--version 0.6.1 --some-other-param', -- runtime_especification
    1000, -- stake,
    10 -- collateral
);

update contracts
set status = 'defined'
where id = X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539';


-- the VN node sees on the blockchain the contract constitution transaction

-- vnc key list
insert into public_key_lists default values;
insert into public_key_items (list_id, public_key) values (1, X'c6fbed3bbf0472bffc9685f7e8859c3c4515b4fe526617ae88f9839862dd8c33');
insert into public_key_items (list_id, public_key) values (1, X'c492fa647980867e414bcca203791325b23202c0e04de14b1c72dec55aa27e7c');
insert into public_key_items (list_id, public_key) values (1, X'542ce0442b230cf13e1d5a6dc69fd2445e4c8b45f0e2ed208df585920cd50543');

-- emergency key list
insert into public_key_lists default values;
insert into public_key_items (list_id, public_key) values (2, X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539');

insert into contract_constitutions (
    contract_id,
    vnc_key_list,
    expiry_timestamp,
    acceptance_quorum,
    initial_reward,
    consensus_algorithm,
    checkpoint_quorum,
    min_checkpoint_frequency,
    committee_change_rules,
    checkpoint_paramenters_change,
    sidechain_metadata_change,
    emergency_key_list
)
values (
    X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539', -- contract_id,
    1, -- vnc_key_list,
    datetime('now', '+1 day'), -- expiry_timestamp,
    100, -- acceptance_quorum,
    100, -- initial_reward,
    'HotStuff', -- consensus_algorithm,
    60, -- checkpoint_quorum,
    100, -- min_checkpoint_frequency,
    'committee_change_rules, format is not clear yet', -- committee_change_rules,
    'checkpoint_paramenters_change, format is not clear yet', -- checkpoint_paramenters_change,
    'sidechain_metadata_change, format is not clear yet', -- sidechain_metadata_change,
    2 -- emergency_key_list
);

update contracts
set status = 'constituted'
where id = X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539';

-- all three members of the VNC accept the contract
insert into contract_acceptances (contract_id, vn_public_key, stake, stake_release_timestamp)
values (
    X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539', -- contract_id,
    X'c6fbed3bbf0472bffc9685f7e8859c3c4515b4fe526617ae88f9839862dd8c33', -- public_key 
    100, -- stake
    datetime('now', '+2 day') -- expiry_timestamp,
);

insert into contract_acceptances (contract_id, vn_public_key, stake, stake_release_timestamp)
values (
    X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539', -- contract_id,
    X'c492fa647980867e414bcca203791325b23202c0e04de14b1c72dec55aa27e7c', -- public_key 
    100, -- stake
    datetime('now', '+2 day') -- expiry_timestamp,
);

insert into contract_acceptances (contract_id, vn_public_key, stake, stake_release_timestamp)
values (
    X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539', -- contract_id,
    X'542ce0442b230cf13e1d5a6dc69fd2445e4c8b45f0e2ed208df585920cd50543', -- public_key 
    100, -- stake
    datetime('now', '+2 day') -- expiry_timestamp,
);

-- the VN node sees on the blockchain the side-chain initialization transaction
-- so it marks the contract as initialized
update contracts
set status = 'initialized'
where id = X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539';

-- the VN node sees on the blockchain some checkpoint transactions
-- so it stores them into the database
insert into contract_checkpoints (contract_id, timestamp, contract_state_commitment, contract_state_uri, checkpoint_number)
values (
    X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539', -- contract_id,
    datetime('now'), -- timestamp, in a real system would be the timestamp of the block containing the checkpoint utxo
    X'0e3cdf292c4ed09a2351e44d62e98795baf2b1ef826798307f9609f827637902', -- contract_state_commitment
    'http://path-to-contract-state', -- contract_state_uri
    1 -- checkpoint_number
);

-- the VN node sees on the blockchain some checkpoint transactions
-- so it stores them into the database
insert into contract_checkpoints (contract_id, timestamp, contract_state_commitment, contract_state_uri, checkpoint_number)
values (
    X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539', -- contract_id,
    datetime('now'), -- timestamp, in a real system would be the timestamp of the block containing the checkpoint utxo
    X'946a9eecef672f085cfad6665b1755def45d03e843d6d2aa8272fd637fcb2082', -- contract_state_commitment
    'http://path-to-contract-state/0', -- contract_state_uri
    1 -- checkpoint_number
);

insert into contract_checkpoints (contract_id, timestamp, contract_state_commitment, contract_state_uri, checkpoint_number)
values (
    X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539', -- contract_id,
    datetime('now'), -- timestamp, in a real system would be the timestamp of the block containing the checkpoint utxo
    X'49695864b5cdf682148a9e4c239d576068831b8e17bbff760baa1fadf45afc8a', -- contract_state_commitment
    'http://path-to-contract-state/1', -- contract_state_uri
    2 -- checkpoint_number
);

insert into contract_checkpoints (contract_id, timestamp, contract_state_commitment, contract_state_uri, checkpoint_number)
values (
    X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539', -- contract_id,
    datetime('now'), -- timestamp, in a real system would be the timestamp of the block containing the checkpoint utxo
    X'b4096a22bb41377ea5c427fc15f8c0371e84ca0443662f9971bbe6ac5094549e', -- contract_state_commitment
    'http://path-to-contract-state/2', -- contract_state_uri
    3 -- checkpoint_number
);

-- the VN node does not see in the blockchain the fourth checkpoint in time
-- so it marks the contract as abandoned
update contracts
set status = 'abandoned'
where id = X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539';

-- the VN node sees the quarantine transaction in the blockchain
-- so it marks the contract as quarantined
update contracts
set status = 'quarantined'
where id = X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539';