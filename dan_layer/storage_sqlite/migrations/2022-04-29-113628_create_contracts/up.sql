create table templates (
    -- should the id be a hash of some sort like the contract_id?
    id integer primary key autoincrement not null,

    -- link to the source code of the template
    source_url varchar(255) unique not null,
    source_hash blob(32) unique not null,
    source_type text check( source_type IN ('source','binary') ) not null,

    version_info varchar(32) null,
    execution_engine_requirements varchar(32) null
);

-- used to reference a transaction output
create table utxos (
    id integer primary key autoincrement not null,

    block_height sqlite3_uint64 not null,
    block_timestamp datetime not null,
    output_hash blob(32) not null,

    -- question: do we need to store more data about the output?

    -- indicates wheter this utxto was spent or not
    spent integer not null default false
);

create index utxos_block_height_index on utxos (block_height);

create table contracts (
    -- contract_id, as per RFC-0312 is calculated as:
    -- H(contract_name || contract specification hash || Initial data hash || Runtime data hash)
    id blob(32) primary key not null,

    -- wheter the VN has accepted this contract
    vn_accepted integer not null default false,

    -- "enum" for all possible stages of the contract lifecycle:
    status text check( status IN (
        'initial',      -- the contract definition transaction is not yet available
        'defined',      -- the contract defintion transaction is published
        'constituted',  -- the contract constitution transaction is published
        'accepted',     -- all the required VNC quorum has accepted and published the acceptance transaction
        'rejected',     -- the required quorum and/or timestamp restrictions were not met 
        'initialized',  -- the side-chain initialization transaction has been published. This is the "execution" phase
        'abandoned',    -- the contract has missed one or more checkpoints
        'quarantined'   -- the contract was abandoned but the emergency key did spent the last checkpoint
    )) not null default 'initial'
);

create table contract_definitions (
    contract_id blob(32) primary key not null,  

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

    -- amount of micro Tari that the asset issuer stakes in order to publish the contract 
    stake sqlite3_uint64 not null,

    -- nominal amount (micro Tari) to prevent spam, and encourages de asset issuear to tidy up after a contract winds down
    collateral sqlite3_uint64 not null,

    -- transaction that holds the contract definition
    utxo_id integer not null,

    foreign key (contract_id) references contracts(id),
    foreign key (template_id) references templates(id),
    foreign key (utxo_id) references utxos(id)
);

-- Holds lists of public keys using One-To-Many relationship ("one" side)
create table public_key_lists (
    id integer primary key autoincrement not null
);

-- Individual public keys in the One-To-Many relationship ("many" side)
create table public_key_items (
    -- we cannot use the public key itself as the table primary key, because a public key can appear in multiple lists
    id integer primary key autoincrement not null,
    list_id integer not null,

    -- public key (ristretto's 32 bytes
    public_key blob(32) not null,
    
    foreign key (list_id) references public_key_lists(id)
);

create table contract_constitutions (
    contract_id blob(32) primary key not null,  

    -- list of public keys of the proposed VNC
    vnc_key_list integer not_null,

    expiry_timestamp datetime not null,

    -- percentage of vnc nodes for the contract acceptance (default 100% of VN)
    acceptance_quorum integer check( 0 >= acceptance_quorum <= 100 ) not null default 100,

    -- optional initial reward that is paid to the VN committee when the UTXO is spent
    initial_reward sqlite3_uint64 null,

    -- side-chain metadata record
    consensus_algorithm varchar(32) null,
    checkpoint_quorum integer check( 0 >= checkpoint_quorum <= 100 ) not null default 100,

    -- checkpoint parameter record
    min_checkpoint_frequency integer not null, -- as number of blocks
    committee_change_rules varchar(255) null, -- the format is not clear

    -- requirements for constitution change record (optional)
    checkpoint_paramenters_change varchar(255) null, -- the format is not clear
    sidechain_metadata_change varchar(255) null, -- the format is not clear

    -- list of emergency public keys that have signing power if the contract is abandoned
    emergency_key_list intgetr not null,

    -- transaction that holds the contract constitution
    utxo_id integer not null,

    foreign key (contract_id) references contracts(id),
    foreign key (vnc_key_list) references public_key_lists(id),
    foreign key (emergency_key_list) references public_key_lists(id),
    foreign key (utxo_id) references utxos(id)
);

-- represents the VN acceptance to a contract
create table contract_acceptances (
    contract_id blob(32) not null,

    -- public key (ristretto's 32 bytes) of the VN member accepting the contract
    vn_public_key blob(32) not null,

    -- the required stake in the contract acceptance transaction
    stake sqlite3_uint64 not null,

    -- the accpetance UTXO time-lock expiration
    stake_release_timestamp datetime not null,

    -- transaction that hold the contract acceptance
    utxo_id integer not null,

    primary key (contract_id, vn_public_key),
    foreign key (contract_id) references contracts(id),
    foreign key (utxo_id) references utxos(id)
);

-- stores the side-chain initializaiton transaction
create table contract_initialization (
    contract_id blob(32) primary key not null,

    -- transaction that hold the initialization transaction
    utxo_id integer not null,
    foreign key (utxo_id) references utxos(id)
);

-- one-to-many relationship with "contracts"
create table contract_checkpoints (
    id integer primary key autoincrement not null,
    contract_id blob(32) not null,

    -- we don't need to store the checkpoint timestamp
    -- as we already have the utxo block timestamp

    --  current contract state, let's assume it's a merkle root
    contract_state_commitment blob(32) not null,

    -- uri to off-chain state or merkle tree
    contract_state_uri varchar(255) null,

    -- strictly increasing by 1 from the previous checkpoint of the contract
    checkpoint_number integer not null,

    -- transaction that hold the checkpoint
    utxo_id integer not null,

    foreign key (contract_id) references contracts(id),
    foreign key (utxo_id) references utxos(id)
);

-- one-to-one relantionship with "contracts"
create table contract_quarantines (
    contract_id blob(32) primary key not null,

    -- transaction that hold the quarantine
    utxo_id integer not null,

    foreign key (contract_id) references contracts(id),
    foreign key (utxo_id) references utxos(id)
);

-- changes to the constituition
create table contract_constitution_changes (
    -- many-to-one with "contracts" 
    id integer primary key autoincrement not null,
    contract_id blob(32) not null,

    -- wheter the VN has accepted this change
    vn_accepted integer not null default false,

    -- list of public keys of the proposed VNC
    vnc_key_list integer not_null,

    -- timestamp expiration for the constitution change acceptance by the VNC
    expiry_timestamp datetime not null,

    -- side-chain metadata record
    consensus_algorithm varchar(32) null,
    checkpoint_quorum integer check( 0 >= checkpoint_quorum <= 100 ) not null default 100,

    -- checkpoint parameter record
    min_checkpoint_frequency integer not null, -- as number of blocks
    committee_change_rules varchar(255) null, -- the format is not clear

    -- "enum" for all possible stages of the constitution change lifecycle
    -- note that constitution changes are still considered "draft", so this part may change
    status text check( status IN (
        'intitial', -- the constitution change proposal transaction is not yet availabe
        'proposed', -- the constitution change proposal transaction was published
        'validated', -- the constitution change validation transaction was published
        'accepted', -- the contract constitution change was accepted by the VNC
        'executed' -- the constitution changes are in effect
    )) not null default 'initial',

    -- transaction that hold the constitution change proposal
    -- question: do we need to store the transaction for each stage?
    utxo_id integer not null,

    foreign key (contract_id) references contracts(id),
    foreign key (utxo_id) references utxos(id)
);

-- represents the VN acceptance to a constitution change acceptance
create table contract_constitution_change_acceptances (
    -- many-to-one relationship "with contract_constitution_change"
    change_id integer not null,

    -- public key (ristretto's 32 bytes) of the VN member accepting the contract
    vn_public_key blob(32) not null,

    -- the required stake in the contract acceptance transaction
    stake sqlite3_uint64 not null,

    -- the accpetance UTXO time-lock expiration
    stake_release_timestamp datetime not null,

    -- transaction that hold the change acceptance
    utxo_id integer not null,

    -- a change can only be accepted once by a VN
    primary key (change_id, vn_public_key),
    foreign key (change_id) references contract_constitution_changes(id),
    foreign key (utxo_id) references utxos(id)
);