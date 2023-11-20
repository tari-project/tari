# Copyright 2022 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

@transaction-info @wallet
Feature: Transaction Info

@long-running
Scenario: Get Transaction Info
    Given I have a seed node NODE
    When I have a stealth SHA3 miner MINER connected to all seed nodes
    When I have wallet WALLET_A connected to all seed nodes
    When I have wallet WALLET_B connected to all seed nodes
    When I have mining node MINER connected to base node NODE and wallet WALLET_A
    When I have a stealth SHA3 miner MINER2 connected to all seed nodes
    # We need to ensure the coinbase lock heights are gone; mine enough blocks
    When mining node MINER mines 4 blocks
    Then all nodes are at height 4
    Then I list all COINBASE transactions for wallet WALLET_A
    When I wait for wallet WALLET_A to have at least 1002000 uT
    When I send 1000000 uT from wallet WALLET_A to wallet WALLET_B at fee 20
    Then wallet WALLET_A detects all transactions are at least Pending
    Then wallet WALLET_B detects all transactions are at least Pending
    Then wallet WALLET_A detects all transactions are at least Completed
    Then wallet WALLET_B detects all transactions are at least Completed
    Then wallet WALLET_A detects all transactions are at least Broadcast
    Then wallet WALLET_B detects all transactions are at least Broadcast
    # This wait is needed to stop next merge mining task from continuing
    When I wait 1 seconds
    When mining node MINER2 mines 1 blocks
    Then all nodes are at height 5
    Then wallet WALLET_A detects all transactions are at least Mined_or_Faux_Unconfirmed
    Then wallet WALLET_B detects all transactions are at least Mined_or_Faux_Unconfirmed
    # This wait is needed to stop base nodes from shutting down
    When I wait 1 seconds
    When mining node MINER2 mines 10 blocks
    Then all nodes are at height 15
    Then wallet WALLET_A detects all transactions as Mined_or_Faux_Confirmed
    Then wallet WALLET_B detects all transactions as Mined_or_Faux_Confirmed
    # This wait is needed to stop base nodes from shutting down
    When I wait 1 seconds
