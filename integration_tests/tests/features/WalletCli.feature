# Copyright 2022 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

@wallet-cli
Feature: Wallet CLI


  Scenario: As a user I want to get balance via command line
    Given I have a base node BASE
    When I have wallet WALLET connected to base node BASE
    When I have SHA3X mining node MINE connected to base node BASE and wallet WALLET
    When mining node MINE mines 5 blocks
    Then I wait for wallet WALLET to have at least 1000000 uT
    Then I get balance of wallet WALLET is at least 1000000 uT via command line

  Scenario: As a user I want to send tari via command line
    Given I have a seed node SEED
    When I have a base node BASE connected to seed SEED
    When I have wallet SENDER connected to base node BASE
    When I have wallet RECEIVER connected to base node BASE
    When I have SHA3X mining node MINE connected to base node BASE and wallet SENDER
    When mining node MINE mines 5 blocks
    Then I wait for wallet SENDER to have at least 1100000 uT
    When I wait 30 seconds
    When I send 1000000 uT from SENDER to RECEIVER via command line
    Then wallet SENDER has at least 1 transactions that are all TRANSACTION_STATUS_BROADCAST and not cancelled
    When mining node MINE mines 5 blocks
    Then I wait for wallet RECEIVER to have at least 1000000 uT

  @long-running
  Scenario: As a user I want to make-it-rain via command line
    Given I have a seed node SEED
    When I have a base node BASE connected to seed SEED
    When I have wallet SENDER connected to base node BASE
    When I have wallet RECEIVER connected to base node BASE
    When I have SHA3X mining node MINE connected to base node BASE and wallet SENDER
    When mining node MINE mines 15 blocks
    Then wallets SENDER should have AT_LEAST 12 spendable coinbase outputs
    Then I stop wallet SENDER
    When I make-it-rain from SENDER rate 10 txns_per_sec duration 1 sec value 8000 uT increment 100 uT to RECEIVER via command line
    Then wallet SENDER has at least 10 transactions that are all TRANSACTION_STATUS_BROADCAST and not cancelled
    Then wallet RECEIVER has at least 10 transactions that are all TRANSACTION_STATUS_BROADCAST and not cancelled
    When mining node MINE mines 5 blocks
    Then I wait for wallet RECEIVER to have at least 84500 uT


  Scenario Outline: As a user I want to coin-split via command line
    Given I have a seed node SEED
    When I have a base node BASE connected to seed SEED
    When I have wallet WALLET connected to base node BASE
    When I have SHA3X mining node MINE connected to base node BASE and wallet WALLET
    When mining node MINE mines 4 blocks
    Then I wait for wallet WALLET to have at least 1100000 uT
    When I wait 30 seconds
    When I do coin split on wallet WALLET to 10000 uT <AMOUNT> coins via command line
    Then wallet WALLET has at least 1 transactions that are all TRANSACTION_STATUS_BROADCAST and not cancelled
    When mining node MINE mines 5 blocks
    Then wallet WALLET has at least 1 transactions that are all TRANSACTION_STATUS_MINED_CONFIRMED and not cancelled
    Then I get count of utxos of wallet WALLET and it's at least <AMOUNT> via command line

    Examples:
      | AMOUNT |
      | 10 |

    @long-running
    Examples:
      | AMOUNT |
      | 100 |
      | 499 |

  Scenario: As a user I want to count utxos via command line
    Given I have a base node BASE
    When I have wallet WALLET connected to base node BASE
    When I have SHA3X mining node MINE connected to base node BASE and wallet WALLET
    When mining node MINE mines 4 blocks
    Then I wait for wallet WALLET to have at least 1000000 uT
    Then I stop wallet WALLET
    Then I get count of utxos of wallet WALLET and it's at least 1 via command line

  Scenario: As a user I want to export utxos via command line
    Given I have a base node BASE
    When I have wallet WALLET connected to base node BASE
    When I have SHA3X mining node MINE connected to base node BASE and wallet WALLET
    When mining node MINE mines 4 blocks
    Then I wait for wallet WALLET to have at least 1000000 uT
    When I export the utxos of wallet WALLET via command line

  Scenario: As a user I want to run whois via command line
    Given I have a base node BASE
    When I have wallet WALLET connected to base node BASE
    Then I run whois BASE on wallet WALLET via command line
