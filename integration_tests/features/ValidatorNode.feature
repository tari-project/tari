# Copyright 2022 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

@dan
Feature: Validator Node
    @critical
    Scenario: Publish contract acceptance
        Given I have a seed node NODE1
        And I have wallet WALLET1 connected to all seed nodes
        And I mine 9 blocks using wallet WALLET1 on NODE1
        And I wait for wallet WALLET1 to have at least 1000000 uT
        And I publish a contract definition DEF1 from file "fixtures/contract_definition.json" on wallet WALLET1 via command line
        And I mine 4 blocks using wallet WALLET1 on NODE1
        And I publish a contract constitution from file "fixtures/contract_constitution.json" on wallet WALLET1 via command line
        And I mine 4 blocks using wallet WALLET1 on NODE1
        And I have a validator node VN1 connected to base node NODE1 and wallet WALLET1
        When I publish a contract acceptance transaction for contract DEF1 for the validator node VN1
        And I mine 9 blocks using wallet WALLET1 on NODE1
        Then wallet WALLET1 will have a successfully mined contract acceptance transaction for contract DEF1

    Scenario: Contract constitution auto acceptance
        Given I have a seed node NODE1
        And I have wallet WALLET1 connected to all seed nodes
        And I mine 9 blocks using wallet WALLET1 on NODE1
        And I wait for wallet WALLET1 to have at least 1000000 uT
        And I have a validator node VN1 connected to base node NODE1 and wallet WALLET1
        And validator node VN1 has "constitution_auto_accept" set to true
        And validator node VN1 has "constitution_management_polling_interval" set to 5
        And validator node VN1 has "constitution_management_polling_interval_in_seconds" set to 5
        And I publish a contract definition DEF1 from file "fixtures/contract_definition.json" on wallet WALLET1 via command line
        And I mine 4 blocks using wallet WALLET1 on NODE1
        When I create a contract constitution COM1 for contract DEF1 from file "fixtures/contract_constitution.json"
        And I add VN1 to the validator committee on COM1
        And I publish the contract constitution COM1 on wallet WALLET1 via command line
        And I mine 9 blocks using wallet WALLET1 on NODE1
        Then wallet WALLET1 will have a successfully mined contract acceptance transaction for contract DEF1

    @critical
    Scenario: Publish contract update proposal acceptance
        Given I have a seed node NODE1
        And I have wallet WALLET1 connected to all seed nodes
        And I mine 9 blocks using wallet WALLET1 on NODE1
        And I wait for wallet WALLET1 to have at least 1000000 uT
        And I publish a contract definition DEF1 from file "fixtures/contract_definition.json" on wallet WALLET1 via command line
        And I mine 4 blocks using wallet WALLET1 on NODE1
        And I publish a contract constitution from file "fixtures/contract_constitution.json" on wallet WALLET1 via command line
        And I mine 4 blocks using wallet WALLET1 on NODE1
        And I publish a contract update proposal from file "fixtures/contract_update_proposal.json" on wallet WALLET1 via command line
        And I mine 4 blocks using wallet WALLET1 on NODE1
        And I have a validator node VN1 connected to base node NODE1 and wallet WALLET1
        When I publish a contract update proposal acceptance transaction for the validator node VN1
        And I mine 9 blocks using wallet WALLET1 on NODE1
        Then wallet WALLET1 will have a successfully mined contract update proposal for contract DEF1