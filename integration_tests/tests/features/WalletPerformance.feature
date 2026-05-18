# Copyright 2024 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

@wallet-performance @long-running
Feature: Wallet Performance


  Scenario: Wallet performance test with 500 transactions after 1000 mined blocks
    Given I have a seed node NODE
    When I have wallet WALLET_A connected to all seed nodes
    When I have wallet WALLET_B connected to all seed nodes
    When I have SHA3X mining node MINER connected to base node NODE and wallet WALLET_A
    When I have SHA3X mining node MINER2 connected to base node NODE and wallet WALLET_B
    When mining node MINER mines 1000 blocks
    When I start benchmark timer balance_sync
    Then I wait for wallet WALLET_A to have at least 1753895088580 uT
    Then I stop benchmark timer balance_sync and log elapsed time
    When I send 500 one-sided transactions of 100000 uT each from wallet WALLET_A to wallet WALLET_B at fee_per_gram 4
    When mining node MINER2 mines 5 blocks
    When I start benchmark timer tx_confirmation
    Then while mining via SHA3 miner MINER2 all transactions in wallet WALLET_A are found to be Mined_or_OneSidedConfirmed
    Then I stop benchmark timer tx_confirmation and log elapsed time
