Feature: Wallet Transfer

  Scenario: As a wallet I want to submit multiple transfers
    Given I have a seed node NODE
    # Add a 2nd node otherwise initial sync will not succeed
    And I have 1 base nodes connected to all seed nodes
    And I have wallet Wallet_A connected to all seed nodes
    And I have a merge mining proxy PROXY connected to NODE and Wallet_A with default config
    And I have mining node MINER connected to base node NODE and wallet Wallet_A
    And I have wallet Wallet_B connected to all seed nodes
    And I have wallet Wallet_C connected to all seed nodes
    When I merge mine 5 blocks via PROXY
    Then all nodes are at height 5
    # Ensure the coinbase lock heights have expired
    And mining node MINER mines 5 blocks
    Then all nodes are at height 10
    When I transfer 50000 uT from Wallet_A to Wallet_B and Wallet_C at fee 100
    And mining node MINER mines 10 blocks
    Then all nodes are at height 20
    Then all wallets detect all transactions as Mined_Confirmed

  Scenario: As a wallet I want to submit transfers to myself
    Given I have a seed node NODE
    # Add a 2nd node otherwise initial sync will not succeed
    And I have 1 base nodes connected to all seed nodes
    And I have wallet Wallet_A connected to all seed nodes
    And I have a merge mining proxy PROXY connected to NODE and Wallet_A with default config
    When I merge mine 5 blocks via PROXY
    Then all nodes are at height 5
    # Ensure the coinbase lock heights have expired
    And I have mining node MINER connected to base node NODE and wallet WC
    And mining node MINER mines 3 blocks
    When I transfer 50000 uT to self from wallet Wallet_A at fee 25
    And mining node MINER mines 5 blocks
    Then all nodes are at height 13
    Then wallet Wallet_A detects all transactions as Mined_Confirmed
