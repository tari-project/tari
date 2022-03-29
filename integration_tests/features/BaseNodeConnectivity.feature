# Copyright 2022 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

@base-node-connectivity
Feature: Base Node Connectivity

    @base-node
    Scenario: Basic connectivity between 2 nodes
        Given I have a seed node SEED_A
        And I have a base node NODE_A connected to all seed nodes
        When I wait for NODE_A to connect to SEED_A
        Then SEED_A is connected to NODE_A

    @base-node @wallet
    Scenario: Basic connectivity between nodes and wallet
        Given I have a seed node SEED_A
        And I have wallet WALLET_A connected to all seed nodes
        Then I wait for WALLET_A to connect to SEED_A
        Then I wait for WALLET_A to have 1 node connections
        Then I wait for WALLET_A to have ONLINE connectivity
        Then SEED_A is connected to WALLET_A

    Scenario: Base node lists heights
        Given I have 1 seed nodes
        And I have a base node N1 connected to all seed nodes
        When I mine 5 blocks on N1
        Then node N1 lists heights 1 to 5

    Scenario: Base node lists headers
        Given I have 1 seed nodes
        And I have a base node BN1 connected to all seed nodes
        When I mine 5 blocks on BN1
        Then node BN1 lists headers 1 to 5 with correct heights
