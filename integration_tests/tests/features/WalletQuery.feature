@wallet-query @wallet
Feature: Wallet Querying

  Scenario: As a wallet I want to query the status of utxos in blocks
    Given I have a seed node WalletSeedA
    When I mine a block on WalletSeedA with coinbase CB1
    Then node WalletSeedA is at height 1

  @critical
  Scenario: As a wallet I want to submit a transaction
    Given I have a seed node SeedA
    When I mine a block on SeedA with coinbase CB1
    When I mine 2 blocks on SeedA
    When I mine 2 blocks on SeedA

  @critical
  Scenario: As a wallet I cannot submit a locked coinbase transaction
    Given I have a seed node SeedA
    When I mine a block on SeedA with coinbase CB1
    When I mine 2 blocks on SeedA
    When I mine 1 blocks on SeedA
