@wallet-query @wallet
Feature: Wallet Querying


  Scenario: As a wallet I want to query the status of utxos in blocks
    Given I have a seed node WalletSeedA
    When I mine a block on WalletSeedA with coinbase CB1
    Then node WalletSeedA is at height 1
    Then the UTXO CB1 has been mined according to WalletSeedA

  @critical
  Scenario: As a wallet I want to submit a transaction
    Given I have a seed node SeedA
    When I mine a block on SeedA with coinbase CB1
    When I mine 2 blocks on SeedA
    When I create a transaction TX1 spending CB1 to UTX1
    When I submit transaction TX1 to SeedA
    Then TX1 is in the mempool
    When I mine 1 blocks on SeedA
    Then the UTXO UTX1 has been mined according to SeedA


  @critical
  Scenario: As a wallet I cannot submit a locked coinbase transaction
    # Using GRPC
    Given I have a seed node SeedA
    When I mine a block on SeedA with coinbase CB1
    When I create a transaction TX1 spending CB1 to UTX1
    When I submit locked transaction TX1 to SeedA
    Then TX1 should not be in the mempool
    When I mine 2 blocks on SeedA
    When I submit transaction TX1 to SeedA
    Then TX1 is in the mempool
    When I mine 1 blocks on SeedA
    Then the UTXO UTX1 has been mined according to SeedA
