Feature: Wallet Transfer

  Scenario: As a wallet I want to submit multiple transfers
    Given I have a seed node NODE
      # Add a 2nd node otherwise initial sync will not succeed
    And I have 1 base nodes connected to all seed nodes
    And I have wallet Wallet_A connected to all seed nodes
    And I have a merge mining proxy PROXY connected to NODE and Wallet_A
    And I have wallet Wallet_B connected to all seed nodes
    And I have wallet Wallet_C connected to all seed nodes
    When I merge mine 2 blocks via PROXY
    Then all nodes are at height 2
      # Ensure the coinbase lock heights have expired
    When I mine 3 blocks on NODE
    Then all nodes are at height 5
    When I transfer 50000 uT from Wallet_A to Wallet_B and Wallet_C at fee 100
    And I mine 5 blocks on NODE
    Then all nodes are at height 10
    Then all wallets detect all transactions as Mined_Confirmed

  Scenario: As a wallet I want to submit transfers to myself
    Given I have a seed node NODE
      # Add a 2nd node otherwise initial sync will not succeed
    And I have 1 base nodes connected to all seed nodes
    And I have wallet Wallet_A connected to all seed nodes
    And I have a merge mining proxy PROXY connected to NODE and Wallet_A
    When I merge mine 2 blocks via PROXY
    Then all nodes are at height 2
      # Ensure the coinbase lock heights have expired
    When I mine 3 blocks on NODE
    When I transfer 50000 uT to self from wallet Wallet_A at fee 25
    And I mine 5 blocks on NODE
    Then all nodes are at height 10
    Then all wallets detect all transactions as Mined_Confirmed
