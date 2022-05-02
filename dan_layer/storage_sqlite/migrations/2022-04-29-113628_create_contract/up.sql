create table templates (
    -- should the id be a hash of some sort like the contract_id?
    id integer primary key autoincrement not null,

    -- link to the source code of the template
    source_url varchar(255) not null,
    source_type text check( source_type IN ('source','binary') ) not null,
    source_hash blob(32) not null,

    version_info varchar(32) null,
    execution_engine_requirements varchar(32) null
);

insert into templates (source_url, source_type, source_hash, version_info, execution_engine_requirements)
values (
    'http://github.com/tari-templates/nft-project', -- source_url
    'source', -- source_type
    X'04e54b3dbb971c87f52f6bb8e2166adc9eea8a63fa8942171731b438fe2bc0f4', -- source_hash
    '1.0.0', -- version_info
    '>=0.6.0 <0.6.4;' -- execution_engine_requirements
);

create table contracts (
    -- contract_id, as per RFC-0312 is calculated as:
    -- H(contract_name || contract specification hash || Initial data hash || Runtime data hash)
    id blob(32) primary key not null,

    -- "enum" for all possible stages of the contract lifecycle:
    status text check( status IN (
        'initial',      -- the contract definition transaction is not yet available
        'defined',      -- the contract defintion transaction is published
        'constituted',  -- the contract constitution transaction is published
        'accepted',     -- the VN has accepted and published the acceptance transaction
        'rejected',     -- the VN has decided to not accept the contract
        'initialized',  -- the side-chain initialization transaction has been published. This is the "execution" phase
        'abandoned',    -- the contract has missed one or more checkpoints
        'quarantined'   -- the contract was abandoned but the emergency key did spent the last checkpoint
    )) not null default 'initial'
);

insert into contracts (id)
values (
    X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539'
);

create table contract_definitions (
    id blob(32) primary key not null,  

    -- RFC-0312 defines it as utf-8 char[32]
    name varchar(32) not null,

    description varchar(255) null,

    -- public key (ristretto's 32 bytes) of the asset issuer
    asset_issuer_key blob(32) not null, 

    template_id integer not null,

    -- the format is not clear for now      
    initialization_arguments blob(32) null,

    -- the format is not clear for now
    -- includes, for example, the version of the runtime and any meta-parameters that the runtime accepts
    runtime_specification varchar(255) null,

    foreign key (id) references contracts(id),
    foreign key (template_id) references templates(id)
);

insert into contract_definitions (
    id, name, description, asset_issuer_key, template_id, initialization_arguments, runtime_specification
)
values (
    X'd28a7a80c9e9f5d29fb7d1aa06e492dc04360b61b6c69743120695ce82f70539', -- id
    'Cool NFT', -- name
    'This is a cool NFT contract', -- description
    X'bec7f50a7307aff31eef64789bcd50e996e4b16b9f974cabef4800add830392f', -- asset_issuer_key
    1, -- template_id, references the "template" table
    X'00000000000000000000000000000001', -- initialization_arguments
    '--version 0.6.1 --some-other-param' -- runtime_especification
);