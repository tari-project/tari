# Copyright 2022 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

@dan
Feature: Validator Node
    @broken
    Scenario: Test committee
        Given I have committee from 4 validator nodes connected
        Then I send instruction successfully with metadata {"issuer" : {"num_clicks" : 1}}
        Then At least 3 out of 4 validator nodes have filled asset data

    @current @broken
    Scenario: Start asset
        Given I have a seed node NODE1
        And I have wallet WALLET1 connected to all seed nodes
        When I mine 9 blocks using wallet WALLET1 on NODE1
        Then I wait for wallet WALLET1 to have at least 1000000 uT
        When I wait 30 seconds
        When I register an NFT asset with committee of 4
        And I mine 3 blocks
        And I create 40 NFTs
        And I mine 3 blocks

    # Broken: needs a contract definition before publishing acceptance, however this is currently not easily done because
    # GRPC methods need to be added and you cannot use the cli for a wallet while that wallet is already running
    @dan @critical @broken
    Scenario: Publish contract acceptance
        Given I have a seed node NODE1
        And I have wallet WALLET1 connected to all seed nodes
        When I mine 9 blocks using wallet WALLET1 on NODE1
        Then I wait for wallet WALLET1 to have at least 1000000 uT
        And I publish a contract definition from file "fixtures/contract_definition.json" on wallet WALLET1 via command line
        When I mine 8 blocks using wallet WALLET1 on NODE1
        Then wallet WALLET1 has at least 1 transactions that are all TRANSACTION_STATUS_MINED_CONFIRMED and not cancelled
        And I have a validator node VN1 connected to base node NODE1 and wallet WALLET1 with "constitiution_auto_accept" set to "false"
        Then I publish a contract acceptance transaction for the validator node VN1
        When I mine 8 blocks using wallet WALLET1 on NODE1
        Then wallet WALLET1 has at least 2 transactions that are all TRANSACTION_STATUS_MINED_CONFIRMED and not cancelled

    @dan
    Scenario: Contract constitution auto acceptance
        Given I have a seed node NODE1
        And I have wallet WALLET1 connected to all seed nodes
        And I mine 9 blocks using wallet WALLET1 on NODE1
        And I wait for wallet WALLET1 to have at least 1000000 uT
        And I have a validator node VN1 connected to base node NODE1 and wallet WALLET1
        And validator node VN1 has "constitution_auto_accept" set to true
        And validator node VN1 has "constitution_management_polling_interval" set to 5
        And I publish a contract definition DEF1 from file "fixtures/contract_definition.json" on wallet WALLET1 via command line
        And I mine 4 blocks using wallet WALLET1 on NODE1
        When I create a contract constitution COM1 for contract DEF1 from file "fixtures/contract_constitution.json"
        And I add VN1 to the validator committee on COM1
        And I publish the contract constitution COM1 on wallet WALLET1 via command line
        And I mine 4 blocks using wallet WALLET1 on NODE1
        Then wallet WALLET1 will have a successfully mined constitution acceptance transaction for contract DEF1

    @dan @critical
    Scenario: Publish contract update proposal acceptance
        Given I have a seed node NODE1
        And I have wallet WALLET1 connected to all seed nodes
        When I mine 9 blocks using wallet WALLET1 on NODE1
        Then I wait for wallet WALLET1 to have at least 1000000 uT
        And I publish a contract definition from file "fixtures/contract_definition.json" on wallet WALLET1 via command line
        When I mine 8 blocks using wallet WALLET1 on NODE1
        Then wallet WALLET1 has at least 1 transactions that are all TRANSACTION_STATUS_MINED_CONFIRMED and not cancelled
        And I have a validator node VN1 connected to base node NODE1 and wallet WALLET1 with "constitiution_auto_accept" set to "false"
        Then I publish a contract update proposal acceptance transaction for the validator node VN1
        When I mine 8 blocks using wallet WALLET1 on NODE1
        Then wallet WALLET1 has at least 2 transactions that are all TRANSACTION_STATUS_MINED_CONFIRMED and not cancelled