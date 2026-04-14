# Copyright 2024 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

@transaction-lock-height @wallet
Feature: Transaction Lock Height

    
Scenario: Coinbase transactions have correct lock_height and transition through locked states
    Given I have a seed node NODE
    When I have wallet WALLET_A connected to all seed nodes
    When I have wallet WALLET_B connected to all seed nodes
    When I have SHA3X mining node MINER connected to base node NODE and wallet WALLET_A
    When I have SHA3X mining node MINER_2 connected to base node NODE and wallet WALLET_B
    # Mine a single block to create a coinbase with maturity lock
    When mining node MINER mines 1 blocks
    Then all nodes are at height 1
    # The coinbase at height 1 should have lock_height = 1 + coinbase_min_maturity (2 on LocalNet = 3)
    # After 1 block, the coinbase is unconfirmed (need 3 confirmations)
    Then wallet WALLET_A detects all transactions are at least Mined_or_OneSidedUnconfirmed
    # Verify that the coinbase transaction has a lock_height > 0
    Then wallet WALLET_A has at least 1 coinbase transactions with lock_height greater than 0
    # Mine enough blocks for the coinbase to be confirmed and unlocked
    # With coinbase_min_maturity=2 and num_confirmations=3, at height 4 (3 confirmations for block 1):
    # tip(4) >= lock_height(3), so it should be fully confirmed (not locked)
    When mining node MINER_2 mines 3 blocks
    Then all nodes are at height 4
    Then wallet WALLET_A detects all transactions are at least Mined
    # This wait is needed to stop base nodes from shutting down
    When I wait 1 seconds
