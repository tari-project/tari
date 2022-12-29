@wallet-cli
Feature: Wallet CLI

  Scenario: As a user I want to change base node for a wallet via command line
    When I have a base node NODE1 connected to all seed nodes
    When I have a base node NODE2 connected to all seed nodes

  Scenario: As a user I want to set and clear custom base node for a wallet via command line
    Given I have a base node NODE1
    When I have a base node NODE2
    When I have wallet WALLET connected to base node NODE1

  Scenario: As a user I want to change password via command line
    Given I have a seed node SEED
    When I have wallet WALLET connected to all seed nodes

  Scenario: As a user I want to get balance via command line
    Given I have a base node BASE
    When I have wallet WALLET connected to base node BASE
    When I have mining node MINE connected to base node BASE and wallet WALLET
    When mining node MINE mines 5 blocks

  @long-running
  Scenario: As a user I want to send tari via command line
    Given I have a seed node SEED
    When I have a base node BASE connected to seed SEED
    When I have wallet SENDER connected to base node BASE
    When I have wallet RECEIVER connected to base node BASE
    When I have mining node MINE connected to base node BASE and wallet SENDER
    When mining node MINE mines 5 blocks
    When I wait 30 seconds
    When mining node MINE mines 5 blocks

  @critical
  Scenario: As a user I want to burn tari via command line
    Given I have a seed node SEED
    When I have a base node BASE connected to seed SEED
    When I have wallet WALLET connected to base node BASE
    When I have mining node MINER connected to base node BASE and wallet WALLET
    When mining node MINER mines 12 blocks
    When I mine 3 blocks on BASE
    Then all nodes are at height 15
    When I mine 5 blocks on BASE
    Then all nodes are at height 20

  @long-running
  Scenario: As a user I want to send one-sided via command line
    Given I have a seed node SEED
    When I have a base node BASE connected to seed SEED
    When I have wallet SENDER connected to base node BASE
    When I have wallet RECEIVER connected to base node BASE
    When I have mining node MINE connected to base node BASE and wallet SENDER
    When mining node MINE mines 5 blocks
    When I wait 30 seconds
    When mining node MINE mines 5 blocks

  @long-running
  Scenario: As a user I want to make-it-rain via command line
    Given I have a seed node SEED
    When I have a base node BASE connected to seed SEED
    When I have wallet SENDER connected to base node BASE
    When I have wallet RECEIVER connected to base node BASE
    When I have mining node MINE connected to base node BASE and wallet SENDER
    When mining node MINE mines 15 blocks
    When I wait 30 seconds
    When mining node MINE mines 5 blocks

  @long-running
  Scenario: As a user I want to coin-split via command line
    Given I have a seed node SEED
    When I have a base node BASE connected to seed SEED
    When I have wallet WALLET connected to base node BASE
    When I have mining node MINE connected to base node BASE and wallet WALLET
    When mining node MINE mines 4 blocks
    When I wait 30 seconds
    When mining node MINE mines 5 blocks

  Scenario: As a user I want to count utxos via command line
    Given I have a base node BASE
    When I have wallet WALLET connected to base node BASE
    When I have mining node MINE connected to base node BASE and wallet WALLET
    When mining node MINE mines 4 blocks

  Scenario: As a user I want to export utxos via command line
    Given I have a base node BASE
    When I have wallet WALLET connected to base node BASE
    When I have mining node MINE connected to base node BASE and wallet WALLET
    When mining node MINE mines 4 blocks

  @flaky
  Scenario: As a user I want to discover-peer via command line
    Given I have a seed node SEED
    When I have wallet WALLET connected to seed node SEED
    When I have a base node BASE1 connected to seed SEED
    When I have a base node BASE2 connected to seed SEED

  Scenario: As a user I want to run whois via command line
    Given I have a base node BASE
