Feature: Wallet Querying

  Scenario: As a wallet I want to query the status of utxos in blocks
    Given I have a seed node WalletSeedA
    When I mine a block on WalletSeedA with coinbase CB1
    Then node WalletSeedA is at height 1
    Then I find that the UTXO CB1 exists according to WalletSeedA
