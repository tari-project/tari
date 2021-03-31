Feature: BlockTemplate

Scenario: Verify UTXO and kernel MMR size in header
    Given I have a seed node SEED_A
    And I have 1 base nodes connected to all seed nodes
    And I have wallet WALLET_A connected to seed node SEED_A
    Then meddling with block template data from node SEED_A for wallet WALLET_A is not allowed
