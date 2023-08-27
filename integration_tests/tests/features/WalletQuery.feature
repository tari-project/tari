# Copyright 2022 The Taiji Project
# SPDX-License-Identifier: BSD-3-Clause

@wallet-query @wallet
Feature: Wallet Querying

  Scenario: As a wallet I want to query the status of utxos in blocks
    Given I have a seed node NODE
    When I have 1 base nodes connected to all seed nodes
    When I have wallet WALLET_A connected to all seed nodes
    When I have mining node MINER connected to base node NODE and wallet WALLET_A
    When mining node MINER mines 5 blocks
    Then all nodes are at height 5
    When I mine 5 blocks on NODE
    Then all wallets detect all transactions as Mined_Confirmed

  @critical
  Scenario: As a wallet I want to submit a transaction
    Given I have a seed node NODE
    When I have 1 base nodes connected to all seed nodes
    When I have wallet WALLET_A with 10T connected to base node NODE
    When I have wallet WALLET_B connected to base node NODE
    When I wait 5 seconds
    When I transfer 5T from WALLET_A to WALLET_B
    When I mine 5 blocks on NODE
    Then all wallets detect all transactions as Mined_Confirmed

