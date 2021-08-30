@wallet-ffi
Feature: Wallet FFI

    Scenario: As a client I want to send Tari to a Public Key
    # It's a subtest of "As a client I want to retrieve a list of transactions I have made and received"

    Scenario: As a client I want to specify a custom fee when I send tari
    # It's a subtest of "As a client I want to retrieve a list of transactions I have made and received"

    Scenario: As a client I want to receive Tari via my Public Key while I am online
    # It's a subtest of "As a client I want to retrieve a list of transactions I have made and received"

    @long-running @broken
    Scenario: As a client I want to receive Tari via my Public Key sent while I am offline when I come back online
        Given I have a base node BASE
        And I have wallet SENDER connected to base node BASE
        And I have mining node MINER connected to base node BASE and wallet SENDER
        And mining node MINER mines 4 blocks
        Then I wait for wallet SENDER to have at least 1000000 uT
        And I have a ffi wallet FFI_WALLET connected to base node BASE
        And I stop wallet FFI_WALLET
        And I wait 5 seconds
        And I send 2000000 uT from wallet SENDER to wallet FFI_WALLET at fee 100
        And I wait 5 seconds
        And I start wallet FFI_WALLET
        And wallet SENDER detects all transactions are at least Broadcast
        And mining node MINER mines 10 blocks
        Then I wait for ffi wallet FFI_WALLET to have at least 1000000 uT

    @long-running
    Scenario: As a client I want to retrieve a list of transactions I have made and received
        Given I have a base node BASE
        And I have wallet SENDER connected to base node BASE
        And I have mining node MINER connected to base node BASE and wallet SENDER
        And mining node MINER mines 4 blocks
        Then I wait for wallet SENDER to have at least 1000000 uT
        And I have a ffi wallet FFI_WALLET connected to base node BASE
        And I send 2000000 uT from wallet SENDER to wallet FFI_WALLET at fee 100
        And wallet SENDER detects all transactions are at least Broadcast
        And mining node MINER mines 10 blocks
        Then I wait for ffi wallet FFI_WALLET to have at least 1000000 uT
        And Check callbacks for finished inbound tx on ffi wallet FFI_WALLET
        And I have wallet RECEIVER connected to base node BASE
        And I send 1000000 uT from ffi wallet FFI_WALLET to wallet RECEIVER at fee 100
        And ffi wallet FFI_WALLET has 1 broadcast transaction
        And mining node MINER mines 4 blocks
        Then I wait for wallet RECEIVER to have at least 1000000 uT
        And Check callbacks for finished outbound tx on ffi wallet FFI_WALLET
        And I have 1 received and 1 send transaction in ffi wallet FFI_WALLET
        And I start STXO validation on wallet FFI_WALLET
        And I start UTXO validation on wallet FFI_WALLET

    # It's just calling the encrypt function, we don't test if it's actually encrypted
    Scenario: As a client I want to be able to protect my wallet with a passphrase
        Given I have a base node BASE
        And I have a ffi wallet FFI_WALLET connected to base node BASE
        And I set passphrase PASSPHRASE of ffi wallet FFI_WALLET

    Scenario: As a client I want to manage contacts
        Given I have a base node BASE
        And I have a ffi wallet FFI_WALLET connected to base node BASE
        And I have wallet WALLET connected to base node BASE
        And I add contact with alias ALIAS and pubkey WALLET to ffi wallet FFI_WALLET
        Then I have contact with alias ALIAS and pubkey WALLET in ffi wallet FFI_WALLET
        When I remove contact with alias ALIAS from ffi wallet FFI_WALLET
        Then I don't have contact with alias ALIAS in ffi wallet FFI_WALLET

    Scenario: As a client I want to set the base node (should be persisted)
        Given I have a base node BASE1
        Given I have a base node BASE2
        And I have a ffi wallet FFI_WALLET connected to base node BASE1
        And I set base node BASE2 for ffi wallet FFI_WALLET
        Then BASE2 is connected to FFI_WALLET
        And I stop wallet FFI_WALLET
        And I wait 5 seconds
        And I start wallet FFI_WALLET
        Then BASE2 is connected to FFI_WALLET

    Scenario: As a client I want to see my public_key, emoji ID, address (whoami)
        Given I have a base node BASE
        And I have a ffi wallet FFI_WALLET connected to base node BASE
        Then I want to get public key of ffi wallet FFI_WALLET
        And I want to get emoji id of ffi wallet FFI_WALLET

    Scenario: As a client I want to get my balance
    # It's a subtest of "As a client I want to retrieve a list of transactions I have made and received"

    @long-running
    Scenario: As a client I want to cancel a transaction
        Given I have a base node BASE
        And I have wallet SENDER connected to base node BASE
        And I have mining node MINER connected to base node BASE and wallet SENDER
        And mining node MINER mines 4 blocks
        Then I wait for wallet SENDER to have at least 1000000 uT
        And I have a ffi wallet FFI_WALLET connected to base node BASE
        And I send 2000000 uT from wallet SENDER to wallet FFI_WALLET at fee 100
        And wallet SENDER detects all transactions are at least Broadcast
        And mining node MINER mines 10 blocks
        Then I wait for ffi wallet FFI_WALLET to have at least 1000000 uT
        And I have wallet RECEIVER connected to base node BASE
        And I stop wallet RECEIVER
        And I send 1000000 uT from ffi wallet FFI_WALLET to wallet RECEIVER at fee 100
        Then I wait for ffi wallet FFI_WALLET to have 1 pending outbound transaction
        Then I cancel all transactions on ffi wallet FFI_WALLET and it will cancel 1 transaction

    @long-running
    Scenario: As a client I want to be able to restore my wallet from seed words
        Given I have a base node BASE
        And I have wallet WALLET connected to base node BASE
        And I have mining node MINER connected to base node BASE and wallet WALLET
        And mining node MINER mines 4 blocks
        Then I wait for wallet WALLET to have at least 1000000 uT
        Then I recover wallet WALLET into ffi wallet FFI_WALLET from seed words on node BASE
        And I wait for recovery of wallet FFI_WALLET to finish
        And I wait for ffi wallet FFI_WALLET to have at least 1000000 uT

    Scenario: As a client I want to be able to initiate TXO and TX validation with the specifed base node.
    # It's a subtest of "As a client I want to retrieve a list of transactions I have made and received"

    Scenario: As a client I want async feedback about the progress of sending and receiving a transaction
    # It's a subtest of "As a client I want to retrieve a list of transactions I have made and received"

    Scenario: As a client I want async feedback about my connection status to the specifed Base Node

    Scenario: As a client I want async feedback about the wallet restoration process
    # As a client I want to be able to restore my wallet from seed words

    Scenario: As a client I want async feedback about TXO and TX validation processes
# It's a subtest of "As a client I want to retrieve a list of transactions I have made and received"
