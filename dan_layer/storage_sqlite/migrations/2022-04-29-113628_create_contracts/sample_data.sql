
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
insert into utxos (block_height, output_hash) -- utxo 1
values (1340, X'9e0bc0a9aa374eab9ae10383ab67c9fadb1e2aa69a1d54555a7dff0670678bda');

insert into contract_definitions (
    contract_id,
    name,
    description,
    asset_issuer_key,
    template_id,
    initialization_arguments,
    runtime_specification,
    stake,
    collateral,
    utxo_id
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
    10, -- collateral
    1 -- utxo_id
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

insert into utxos (block_height, output_hash) -- utxo 2
values (1350, X'cba8d3a39041d4164e60270c6281f81be92d29656bec527211d87de66571f403');

insert into contract_constitutions (
    contract_id,
    vnc_key_list,
    expiration_window,
    acceptance_quorum,
    initial_reward,
    consensus_algorithm,
    quorum_committee_count,
    quorum_required_acceptances,
    quorum_required_checkpoint_votes,
    min_checkpoint_frequency,
    checkpoint_paramenters_change,
    sidechain_metadata_change,
    emergency_key_list,
    utxo_id
)
values (
    X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539', -- contract_id,
    1, -- vnc_key_list,
    100, -- expiration_window,
    100, -- acceptance_quorum,
    100, -- initial_reward,
    'HotStuff', -- consensus_algorithm,
    3, -- quorum_committee_count,
    3, -- quorum_required_acceptances,
    3, -- quorum_required_checkpoint_votes
    100, -- min_checkpoint_frequency,
    'checkpoint_paramenters_change, format is not clear yet', -- checkpoint_paramenters_change,
    'sidechain_metadata_change, format is not clear yet', -- sidechain_metadata_change,
    2, -- emergency_key_list
    2 -- utxo_id
);

update contracts
set status = 'constituted'
where id = X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539';

-- all three members of the VNC accept the contract
insert into utxos (block_height, output_hash) -- utxo 3
values (1360, X'd034a904e224b629947dac2110cf9900b655e6934aa735d5144f92a3399785fc');

insert into contract_acceptances (contract_id, vn_public_key, stake, stake_release_window, utxo_id)
values (
    X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539', -- contract_id,
    X'c6fbed3bbf0472bffc9685f7e8859c3c4515b4fe526617ae88f9839862dd8c33', -- public_key 
    100, -- stake
    100, -- expiration_window
    3 -- utxo_id
);

insert into utxos (block_height, output_hash) -- utxo 4
values (1370, X'07e9bba1c63a7a01f935ee06d931c453e0864fc0fcc11ec445a8af60a36cc6c8');

insert into contract_acceptances (contract_id, vn_public_key, stake, stake_release_window, utxo_id)
values (
    X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539', -- contract_id,
    X'c492fa647980867e414bcca203791325b23202c0e04de14b1c72dec55aa27e7c', -- public_key 
    100, -- stake
    100, -- expiration_window
    4 -- utxo_id
);

insert into utxos (block_height, output_hash) -- utxo 5
values (1380, X'aecef364029f6f3f008b4f2b87b9a1c27e596a74062b83cad24fab73cce4f1f0');

insert into contract_acceptances (contract_id, vn_public_key, stake, stake_release_window, utxo_id)
values (
    X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539', -- contract_id,
    X'542ce0442b230cf13e1d5a6dc69fd2445e4c8b45f0e2ed208df585920cd50543', -- public_key 
    100, -- stake
    100, -- stake_release_window
    5 -- utxo_id
);

-- the VN node sees on the blockchain the side-chain initialization transaction
-- so it marks the contract as initialized
insert into utxos (block_height, output_hash) -- utxo 6
values (1390, X'bb4d3bd125603e48cf30c795c427afd9da53f3a70482aa98ca3b8bbe1980d021');

insert into contract_initialization(contract_id, utxo_id)
values (X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539', 6);

update contracts
set status = 'initialized'
where id = X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539';

-- the VN node sees on the blockchain some checkpoint transactions
-- so it stores them into the database

insert into utxos (block_height, output_hash) -- utxo 7
values (1400, X'55fdec963805de594b61b2c1692cadc2c1dfb844d6ac10c5bfed33c842087b2e');

insert into contract_checkpoints (contract_id, contract_state_commitment, contract_state_uri, checkpoint_number, utxo_id)
values (
    X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539', -- contract_id,
    X'0e3cdf292c4ed09a2351e44d62e98795baf2b1ef826798307f9609f827637902', -- contract_state_commitment
    'http://path-to-contract-state/0', -- contract_state_uri
    0, -- checkpoint_number
    7 -- utxo_id
);

insert into utxos (block_height, output_hash) -- utxo 8
values (1410, X'4103f0a4e707b1c7bebbc42809ab0ace8dd3f56d844d7903bfe9f95a2ccc6972');

insert into contract_checkpoints (contract_id, contract_state_commitment, contract_state_uri, checkpoint_number, utxo_id)
values (
    X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539', -- contract_id,
    X'946a9eecef672f085cfad6665b1755def45d03e843d6d2aa8272fd637fcb2082', -- contract_state_commitment
    'http://path-to-contract-state/1', -- contract_state_uri
    1, -- checkpoint_number
    8 -- utxo_id
);

insert into utxos (block_height, output_hash) -- utxo 9
values (1420, X'3141d9c749e88ee5550192703c3e025a9b3446ec543f17a923cb24e7b61dfece');

insert into contract_checkpoints (contract_id, contract_state_commitment, contract_state_uri, checkpoint_number, utxo_id)
values (
    X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539', -- contract_id,
    X'49695864b5cdf682148a9e4c239d576068831b8e17bbff760baa1fadf45afc8a', -- contract_state_commitment
    'http://path-to-contract-state/2', -- contract_state_uri
    2, -- checkpoint_number
    9 -- utxo_id
);

-- the VN node does not see in the blockchain the fourth checkpoint in time
-- so it marks the contract as abandoned
update contracts
set status = 'abandoned'
where id = X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539';

-- the VN node sees the quarantine transaction in the blockchain
-- so it marks the contract as quarantined
insert into utxos (block_height, output_hash) -- utxo 10
values (1430, X'74bbcf773c82f8b9b1a44138238d135520fe7a0fb898af5c18292a6fe9d23eb8');

insert into contract_quarantines (contract_id, utxo_id)
values (X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539', 10);

update contracts
set status = 'quarantined'
where id = X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539';