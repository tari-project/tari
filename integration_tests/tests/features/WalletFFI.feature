# Copyright 2022 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

@wallet-ffi
Feature: Wallet FFI
    Scenario: As a client I want to see my whoami info
        Given I have a base node BASE
        Given I have a ffi wallet FFI_WALLET connected to base node BASE
        Then I want to get public key of ffi wallet FFI_WALLET
        And I want to get emoji id of ffi wallet FFI_WALLET
        And I stop ffi wallet FFI_WALLET

    Scenario: As a client I want to be able to restore my ffi wallet from seed words
        Given I have a base node BASE
        When I have wallet SPECTATOR connected to base node BASE
        When I have mining node MINER connected to base node BASE and wallet SPECTATOR
        When mining node MINER mines 10 blocks
        Then I wait for wallet SPECTATOR to have at least 1000000 uT
        Then I recover wallet SPECTATOR into ffi wallet FFI_WALLET from seed words on node BASE
        And I wait for ffi wallet FFI_WALLET to have at least 1000000 uT
        And I stop ffi wallet FFI_WALLET

    @critical
    Scenario: As a client I want to retrieve the mnemonic word list for a given language
        Then I retrieve the mnemonic word list for CHINESE_SIMPLIFIED
        Then I retrieve the mnemonic word list for ENGLISH
        Then I retrieve the mnemonic word list for FRENCH
        Then I retrieve the mnemonic word list for ITALIAN
        Then I retrieve the mnemonic word list for JAPANESE
        Then I retrieve the mnemonic word list for KOREAN
        Then I retrieve the mnemonic word list for SPANISH

    Scenario: As a client I want to set the base node
        Given I have a base node BASE1
        And I have a base node BASE2
        Given I have a ffi wallet FFI_WALLET connected to base node BASE1
        Then I wait for ffi wallet FFI_WALLET to connect to BASE1
        Given I set base node BASE2 for ffi wallet FFI_WALLET
        Then I wait for ffi wallet FFI_WALLET to connect to BASE2
        And I stop ffi wallet FFI_WALLET

    Scenario: As a client I want to cancel a transaction
        Given I have a base node BASE
        When I have wallet SENDER connected to base node BASE
        And I have a ffi wallet FFI_WALLET connected to base node BASE
        When I have mining node MINER connected to base node BASE and wallet SENDER
        When mining node MINER mines 10 blocks
        Then I wait for wallet SENDER to have at least 1000000 uT
        And I send 2000000 uT without waiting for broadcast from wallet SENDER to wallet FFI_WALLET at fee 20
        Then ffi wallet FFI_WALLET detects AT_LEAST 1 ffi transactions to be TRANSACTION_STATUS_BROADCAST
        And wallet SENDER detects all transactions are at least Broadcast
        When mining node MINER mines 10 blocks
        Then I wait for ffi wallet FFI_WALLET to have at least 1000000 uT
        When I have wallet RECEIVER connected to base node BASE
        And I stop wallet RECEIVER
        And I send 1000000 uT from ffi wallet FFI_WALLET to wallet RECEIVER at fee 20
        Then I wait for ffi wallet FFI_WALLET to have 1 pending outbound transaction
        Then I cancel all outbound transactions on ffi wallet FFI_WALLET and it will cancel 1 transaction
        Then I wait for ffi wallet FFI_WALLET to have 0 pending outbound transaction
        And I stop ffi wallet FFI_WALLET

    Scenario: As a client I want to manage contacts
        Given I have a base node BASE
        And I have a ffi wallet FFI_WALLET connected to base node BASE
        When I have wallet WALLET connected to base node BASE
        And I add contact with alias ALIAS and address of WALLET to ffi wallet FFI_WALLET
        Then I have contact with alias ALIAS and address of WALLET in ffi wallet FFI_WALLET
        When I remove contact with alias ALIAS from ffi wallet FFI_WALLET
        Then I don't have contact with alias ALIAS in ffi wallet FFI_WALLET
        And I stop ffi wallet FFI_WALLET

    @critical
    Scenario: As a client I want to receive contact liveness events
        Given I have a seed node SEED
        # Contact liveness is based on P2P messaging; ensure connectivity by forcing 'DirectOnly'
        And I have non-default wallet WALLET1 connected to all seed nodes using DirectOnly
        And I have non-default wallet WALLET2 connected to all seed nodes using DirectOnly
        And I have a ffi wallet FFI_WALLET connected to seed node SEED
        # Start the contact liveness pings by adding contacts to the FFI wallet
        When I add contact with alias ALIAS1 and address of WALLET1 to ffi wallet FFI_WALLET
        And I add contact with alias ALIAS2 and address of WALLET2 to ffi wallet FFI_WALLET
        # Do some mining and send transactions to force P2P discovery
        And I have mining node MINER1 connected to base node SEED and wallet WALLET1
        And I have mining node MINER2 connected to base node SEED and wallet WALLET2
        And mining node MINER1 mines 1 blocks
        And mining node MINER2 mines 5 blocks
        Then I wait for wallet WALLET1 to have at least 100000000 uT
        And I wait for wallet WALLET2 to have at least 100000000 uT
        When I send 100000000 uT without waiting for broadcast from wallet WALLET1 to wallet FFI_WALLET at fee 20
        And I send 100000000 uT without waiting for broadcast from wallet WALLET2 to wallet FFI_WALLET at fee 20
        # If the FFI wallet can send the transactions, P2P connectivity has been established
        Then I wait for ffi wallet FFI_WALLET to have at least 2 contacts to be Online
        And I stop ffi wallet FFI_WALLET

    @critical
    Scenario: As a client I want to retrieve a list of transactions I have made and received
        Given I have a seed node SEED
        When I have a base node BASE1 connected to all seed nodes
        When I have wallet SENDER connected to base node BASE1
        And I have a ffi wallet FFI_WALLET connected to base node BASE1
        # Force some P2P discovery with contact liveness
        When I add contact with alias ALIAS1 and address of SENDER to ffi wallet FFI_WALLET
        When I have wallet RECEIVER connected to base node BASE1
        When I have mining node MINER connected to base node BASE1 and wallet SENDER
        When mining node MINER mines 10 blocks
        Then all nodes are at height 10
        Then I wait for wallet SENDER to have at least 2000000 uT
        And I send 2000000 uT from wallet SENDER to wallet FFI_WALLET at fee 20
        Then ffi wallet FFI_WALLET detects AT_LEAST 1 ffi transactions to be TRANSACTION_STATUS_BROADCAST
        When mining node MINER mines 10 blocks
        Then all nodes are at height 20
        Then I wait for ffi wallet FFI_WALLET to have at least 1000000 uT
        And I send 1000000 uT from ffi wallet FFI_WALLET to wallet RECEIVER at fee 20
        Then ffi wallet FFI_WALLET detects AT_LEAST 2 ffi transactions to be TRANSACTION_STATUS_BROADCAST
        Then wallet RECEIVER has at least 1 transactions that are all TRANSACTION_STATUS_BROADCAST and not cancelled
        Then I wait until base node BASE1 has 1 unconfirmed transactions in its mempool
        Then I wait until base node SEED has 1 unconfirmed transactions in its mempool
        # The broadcast check does not include delivery; create some holding points to ensure it was received
        When mining node MINER mines 4 blocks
        Then all nodes are at height 24
        Then ffi wallet FFI_WALLET detects AT_LEAST 2 ffi transactions to be TRANSACTION_STATUS_MINED
#        When mining node MINER mines 6 blocks
        Then I wait for wallet RECEIVER to have at least 1000000 uT
        And I have 1 received and 1 send transaction in ffi wallet FFI_WALLET
        And I start TXO validation on ffi wallet FFI_WALLET
        And I start TX validation on ffi wallet FFI_WALLET
        Then I wait for ffi wallet FFI_WALLET to receive 2 mined
        Then I want to view the transaction kernels for completed transactions in ffi wallet FFI_WALLET
        And I stop ffi wallet FFI_WALLET


    @critical @broken
    Scenario: As a client I want to receive Tari via my Public Key sent while I am offline when I come back online
        Given I have a seed node SEED
        When I have a base node BASE1 connected to all seed nodes
        When I have wallet SENDER connected to base node BASE1
        And I have a ffi wallet FFI_WALLET connected to base node SEED

        # Force some P2P discovery with contact liveness
        When I add contact with alias ALIAS1 and address of SENDER to ffi wallet FFI_WALLET

        # Established comms by funding the FFI wallet
        When I have mining node MINER connected to base node BASE1 and wallet SENDER
        When mining node MINER mines 10 blocks
        Then all nodes are at height 10
        Then I wait for wallet SENDER to have at least 129239250000 uT
        And I send 1000000 uT from wallet SENDER to wallet FFI_WALLET at fee 5
        Then wallet SENDER has at least 1 transactions that are all TRANSACTION_STATUS_BROADCAST and not cancelled
        Then ffi wallet FFI_WALLET detects AT_LEAST 1 ffi transactions to be TRANSACTION_STATUS_BROADCAST
        When mining node MINER mines 10 blocks
        Then all nodes are at height 20
        Then ffi wallet FFI_WALLET detects AT_LEAST 1 ffi transactions to be TRANSACTION_STATUS_MINED
        Then I wait for ffi wallet FFI_WALLET to have at least 1000000 uT

        # We have established comms, so now we can go offline and receive a transaction while offline
        And I stop ffi wallet FFI_WALLET
        And I send 1000000 uT without waiting for broadcast from wallet SENDER to wallet FFI_WALLET at fee 20

        # Let's restart the wallet and see if it can receive the offline transaction
        And I restart ffi wallet FFI_WALLET connected to base node BASE1
        When I add contact with alias ALIAS2 and address of SENDER to ffi wallet FFI_WALLET
        # BROKEN
        And I send 1000000 uT from wallet SENDER to wallet FFI_WALLET at fee 5
        Then ffi wallet FFI_WALLET detects AT_LEAST 1 ffi transactions to be TRANSACTION_STATUS_BROADCAST
        When mining node MINER mines 2 blocks
        Then all nodes are at height 22
        Then ffi wallet FFI_WALLET detects AT_LEAST 2 ffi transactions to be TRANSACTION_STATUS_BROADCAST
        When mining node MINER mines 10 blocks
        Then all nodes are at height 32
        Then I wait for ffi wallet FFI_WALLET to receive 1 mined
        Then I wait for ffi wallet FFI_WALLET to have at least 3000000 uT
        And I stop ffi wallet FFI_WALLET

    @critical
    Scenario: As a client I want to send a one-sided transaction
        Given I have a seed node SEED
        When I have a base node BASE1 connected to all seed nodes
        When I have wallet SENDER connected to base node BASE1
        And I have a ffi wallet FFI_WALLET connected to base node SEED
        When I have wallet RECEIVER connected to base node BASE1

        # Force some P2P discovery with contact liveness
        When I add contact with alias ALIAS1 and address of SENDER to ffi wallet FFI_WALLET
        When I add contact with alias ALIAS2 and address of RECEIVER to ffi wallet FFI_WALLET

        # Fund the FFI wallet
        When I have mining node MINER connected to base node BASE1 and wallet SENDER
        When mining node MINER mines 10 blocks
        Then all nodes are at height 10
        Then I wait for wallet SENDER to have at least 129239250000 uT
        And I send 2400000 uT from wallet SENDER to wallet FFI_WALLET at fee 5
        And I send 2400000 uT from wallet SENDER to wallet FFI_WALLET at fee 5
        Then wallet SENDER has at least 2 transactions that are all TRANSACTION_STATUS_BROADCAST and not cancelled
        Then ffi wallet FFI_WALLET detects AT_LEAST 2 ffi transactions to be TRANSACTION_STATUS_BROADCAST
        When mining node MINER mines 10 blocks
        Then all nodes are at height 20
        Then ffi wallet FFI_WALLET detects AT_LEAST 2 ffi transactions to be TRANSACTION_STATUS_MINED
        Then I wait for ffi wallet FFI_WALLET to have at least 4000000 uT

        # The FFI wallet now has funds to send a one-sided transaction
        And I send 1000000 uT from ffi wallet FFI_WALLET to wallet RECEIVER at fee 5 via one-sided transactions
        Then ffi wallet FFI_WALLET detects AT_LEAST 3 ffi transactions to be TRANSACTION_STATUS_BROADCAST
        When mining node MINER mines 2 blocks
        Then all nodes are at height 22
        Then wallet RECEIVER has at least 1 transactions that are all TRANSACTION_STATUS_ONE_SIDED_UNCONFIRMED and not cancelled
        When mining node MINER mines 5 blocks
        Then all nodes are at height 27
        Then wallet RECEIVER has at least 1 transactions that are all TRANSACTION_STATUS_ONE_SIDED_CONFIRMED and not cancelled
        And I stop ffi wallet FFI_WALLET

    @critical
    Scenario: As a client I want to receive a one-sided transaction
        Given I have a seed node SEED
        When I have a base node BASE1 connected to all seed nodes
        When I have a base node BASE2 connected to all seed nodes
        When I have wallet SENDER connected to base node BASE1
        And I have a ffi wallet FFI_RECEIVER connected to base node BASE2
        When I have mining node MINER connected to base node BASE1 and wallet SENDER
        When mining node MINER mines 10 blocks
        Then I wait for wallet SENDER to have at least 5000000 uT
        Then I send a one-sided transaction of 1000000 uT from SENDER to FFI_RECEIVER at fee 20
        When mining node MINER mines 2 blocks
        Then all nodes are at height 12
        Then ffi wallet FFI_RECEIVER detects AT_LEAST 1 ffi transactions to be TRANSACTION_STATUS_ONE_SIDED_UNCONFIRMED
        And I send 1000000 uT from wallet SENDER to wallet FFI_RECEIVER at fee 20
        Then ffi wallet FFI_RECEIVER detects AT_LEAST 1 ffi transactions to be TRANSACTION_STATUS_BROADCAST
        When mining node MINER mines 5 blocks
        Then all nodes are at height 17
        Then ffi wallet FFI_RECEIVER detects AT_LEAST 1 ffi transactions to be TRANSACTION_STATUS_ONE_SIDED_CONFIRMED
        And I stop ffi wallet FFI_RECEIVER

    Scenario: As a client I want to get fee per gram stats
        Given I have a base node BASE
        When I have wallet WALLET_A connected to base node BASE
        When I have wallet WALLET_B connected to base node BASE
        When I have mining node MINER connected to base node BASE and wallet WALLET_A
        When mining node MINER mines 7 blocks
        Then I wait for wallet WALLET_A to have at least 10000000 uT
        And I have a ffi wallet FFI_WALLET connected to base node BASE
        And The fee per gram stats for FFI_WALLET are 1, 1, 1
        And I send 1000000 uT from wallet WALLET_A to wallet WALLET_B at fee 20
        And The fee per gram stats for FFI_WALLET are 20, 20, 20
        And I send 1000000 uT from wallet WALLET_A to wallet WALLET_B at fee 40
        And The fee per gram stats for FFI_WALLET are 20, 30, 40
        And I send 1000000 uT from wallet WALLET_A to wallet WALLET_B at fee 60
        And The fee per gram stats for FFI_WALLET are 20, 40, 60
        When mining node MINER mines 1 blocks
        And The fee per gram stats for FFI_WALLET are 1, 1, 1
