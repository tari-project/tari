# Copyright 2022 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

Feature: Base Node Connectivity

  @base-node
  Scenario: Basic connectivity between 2 nodes
    Given I have a seed node SEED_A
    When I have a base node NODE_A connected to all seed nodes
    When I wait for NODE_A to connect to SEED_A

  @base-node @wallet
  Scenario: Basic connectivity between nodes and wallet
    Given I have a seed node SEED_A
    When I have wallet WALLET_A connected to all seed nodes
    Then I wait for WALLET_A to connect to SEED_A
    Then I wait for WALLET_A to have 1 node connections
    Then I wait for WALLET_A to have ONLINE connectivity

  @base-node @wallet
  Scenario: Basic mining
    Given I have a seed node NODE
    When I have wallet WALLET connected to all seed nodes
    When I have mining node MINER connected to base node NODE and wallet WALLET
    Given mining node MINER mines 1 blocks
    Then node NODE is at height 1

  Scenario: Basic mining with templates
    Given I have a base node NODE
    When I mine 2 blocks on NODE
    Then node NODE is at height 2
    Then all nodes are at height 2

  Scenario: Base node lists heights
    Given I have a seed node N1
    When I mine 5 blocks on N1
    Then node N1 lists heights 1 to 5


  Scenario: Base node lists headers
    Given I have a seed node BN1
    When I mine 5 blocks on BN1
    Then node BN1 lists headers 1 to 5 with correct heights
