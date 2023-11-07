# Copyright 2022 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

@wallet-cli
Feature: Wallet CLI

    Scenario: As a user I want to change base node for a wallet via command line
        When I have a base node NODE1 connected to all seed nodes
        When I have a base node NODE2 connected to all seed nodes
        When I have wallet WALLET connected to base node NODE1
        Then I change base node of WALLET to NODE2 via command line

    Scenario: As a user I want to set and clear custom base node for a wallet via command line
        Given I have a base node NODE1
        When I have a base node NODE2
        When I have wallet WALLET connected to base node NODE1
        Then I set custom base node of WALLET to NODE2 via command line
        When I clear custom base node of wallet WALLET via command line

    Scenario: As a user I want to get balance via command line
        Given I have a base node BASE
        When I have wallet WALLET connected to base node BASE
        When I have mining node MINE connected to base node BASE and wallet WALLET
        When mining node MINE mines 5 blocks
        Then I wait for wallet WALLET to have at least 1000000 uT
        Then I get balance of wallet WALLET is at least 1000000 uT via command line

    @long-running
    Scenario: As a user I want to send tari via command line
        Given I have a seed node SEED
        When I have a base node BASE connected to seed SEED
        When I have wallet SENDER connected to base node BASE
        When I have wallet RECEIVER connected to base node BASE
        When I have mining node MINE connected to base node BASE and wallet SENDER
        When mining node MINE mines 5 blocks
        Then I wait for wallet SENDER to have at least 1100000 uT
        When I wait 30 seconds
        When I send 1000000 uT from SENDER to RECEIVER via command line
        Then wallet SENDER has at least 1 transactions that are all TRANSACTION_STATUS_BROADCAST and not cancelled
        Then wallet RECEIVER has at least 1 transactions that are all TRANSACTION_STATUS_BROADCAST and not cancelled
        When mining node MINE mines 5 blocks
        Then I wait for wallet RECEIVER to have at least 1000000 uT

    #This is flaky, passes on local run time, but fails CI
    @critical @broken
    Scenario: As a user I want to burn tari via command line
        Given I have a seed node SEED
        When I have a base node BASE connected to seed SEED
        When I have wallet WALLET connected to base node BASE
        When I have mining node MINER connected to base node BASE and wallet WALLET
        When mining node MINER mines 12 blocks
        When I mine 3 blocks on BASE
        Then all nodes are at height 15
        When I wait for wallet WALLET to have at least 221552530060 uT
        When I create a burn transaction of 201552500000 uT from WALLET via command line
        When I mine 5 blocks on BASE
        Then all nodes are at height 20
        Then I get balance of wallet WALLET is at least 20000000000 uT via command line
        #
    @long-running
    Scenario: As a user I want to send one-sided via command line
        Given I have a seed node SEED
        When I have a base node BASE connected to seed SEED
        When I have wallet SENDER connected to base node BASE
        When I have wallet RECEIVER connected to base node BASE
        When I have mining node MINE connected to base node BASE and wallet SENDER
        When mining node MINE mines 5 blocks
        Then I wait for wallet SENDER to have at least 1100000 uT
        When I wait 30 seconds
        Then I stop wallet SENDER
        Then I send one-sided 1000000 uT from SENDER to RECEIVER via command line
        Then wallet SENDER has at least 1 transactions that are all TRANSACTION_STATUS_BROADCAST and not cancelled
        When mining node MINE mines 5 blocks
        Then I wait for wallet RECEIVER to have at least 1000000 uT

    @long-running
    Scenario: As a user I want to make-it-rain via command line
        Given I have a seed node SEED
        When I have a base node BASE connected to seed SEED
        When I have wallet SENDER connected to base node BASE
        When I have wallet RECEIVER connected to base node BASE
        When I have mining node MINE connected to base node BASE and wallet SENDER
        When mining node MINE mines 15 blocks
        Then wallets SENDER should have AT_LEAST 12 spendable coinbase outputs
        When I wait 30 seconds
        Then I stop wallet SENDER
        When I make it rain from wallet SENDER 1 tx per sec 10 sec 8000 uT 100 increment to RECEIVER via command line
        Then wallet SENDER has at least 10 transactions that are all TRANSACTION_STATUS_BROADCAST and not cancelled
        Then wallet RECEIVER has at least 10 transactions that are all TRANSACTION_STATUS_BROADCAST and not cancelled
        When mining node MINE mines 5 blocks
        Then I wait for wallet RECEIVER to have at least 84500 uT

    @long-running
    Scenario: As a user I want to coin-split via command line
        Given I have a seed node SEED
        When I have a base node BASE connected to seed SEED
        When I have wallet WALLET connected to base node BASE
        When I have mining node MINE connected to base node BASE and wallet WALLET
        When mining node MINE mines 4 blocks
        Then I wait for wallet WALLET to have at least 1100000 uT
        When I wait 30 seconds
        When I do coin split on wallet WALLET to 10000 uT 10 coins via command line
        Then wallet WALLET has at least 1 transactions that are all TRANSACTION_STATUS_BROADCAST and not cancelled
        When mining node MINE mines 5 blocks
        Then wallet WALLET has at least 1 transactions that are all TRANSACTION_STATUS_MINED_CONFIRMED and not cancelled
        Then I get count of utxos of wallet WALLET and it's at least 10 via command line

    @long-running
    Scenario: As a user I want to large coin-split via command line
        Given I have a seed node SEED
        When I have a base node BASE connected to seed SEED
        When I have wallet WALLET connected to base node BASE
        When I have mining node MINE connected to base node BASE and wallet WALLET
        When mining node MINE mines 4 blocks
        Then I wait for wallet WALLET to have at least 1100000 uT
        When I wait 30 seconds
        When I do coin split on wallet WALLET to 10000 uT 499 coins via command line
        Then wallet WALLET has at least 1 transactions that are all TRANSACTION_STATUS_BROADCAST and not cancelled
        When mining node MINE mines 5 blocks
        Then wallet WALLET has at least 1 transactions that are all TRANSACTION_STATUS_MINED_CONFIRMED and not cancelled
        Then I get count of utxos of wallet WALLET and it's at least 499 via command line

    Scenario: As a user I want to count utxos via command line
        Given I have a base node BASE
        When I have wallet WALLET connected to base node BASE
        When I have mining node MINE connected to base node BASE and wallet WALLET
        When mining node MINE mines 4 blocks
        Then I wait for wallet WALLET to have at least 1000000 uT
        Then I stop wallet WALLET
        Then I get count of utxos of wallet WALLET and it's at least 1 via command line

    Scenario: As a user I want to export utxos via command line
        Given I have a base node BASE
        When I have wallet WALLET connected to base node BASE
        When I have mining node MINE connected to base node BASE and wallet WALLET
        When mining node MINE mines 4 blocks
        Then I wait for wallet WALLET to have at least 1000000 uT
        When I export the utxos of wallet WALLET via command line

    @flaky
    Scenario: As a user I want to discover-peer via command line
        Given I have a seed node SEED
        When I have wallet WALLET connected to seed node SEED
        When I have a base node BASE1 connected to seed SEED
        When I have a base node BASE2 connected to seed SEED
        When I discover peer BASE2 on wallet WALLET via command line
        Then WALLET is connected to BASE2

    Scenario: As a user I want to run whois via command line
        Given I have a base node BASE
        When I have wallet WALLET connected to base node BASE
        Then I run whois BASE on wallet WALLET via command line
