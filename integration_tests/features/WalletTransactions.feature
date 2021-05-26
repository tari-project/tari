Feature: Wallet Transactions

  @critical
  Scenario: Wallet sending and receiving one-sided transactions
    Given I have a seed node NODE
    And I have 1 base nodes connected to all seed nodes
    And I have wallet WALLET_A connected to all seed nodes
    And I have a merge mining proxy PROXY connected to NODE and WALLET_A with default config
    When I merge mine 15 blocks via PROXY
    Then all nodes are at height 15
    When I wait for wallet WALLET_A to have at least 55000000000 uT
    And I have wallet WALLET_B connected to all seed nodes
    Then I send a one-sided transaction of 1000000 uT from WALLET_A to WALLET_B at fee 100
    Then I send a one-sided transaction of 1000000 uT from WALLET_A to WALLET_B at fee 100
    When I merge mine 5 blocks via PROXY
    Then all nodes are at height 20
    Then I wait for wallet WALLET_B to have at least 2000000 uT
    # Spend one of the recovered UTXOs to self in a standard MW transaction
    Then I send 900000 uT from wallet WALLET_B to wallet WALLET_B at fee 100
    Then I wait for wallet WALLET_B to have less than 1100000 uT
    When I merge mine 5 blocks via PROXY
    Then all nodes are at height 25
    Then I wait for wallet WALLET_B to have at least 1900000 uT
    # Make a one-sided payment to a new wallet that is big enough to ensure the second recovered output is spent
    And I have wallet WALLET_C connected to all seed nodes
    Then I send a one-sided transaction of 1500000 uT from WALLET_B to WALLET_C at fee 100
    Then I wait for wallet WALLET_B to have less than 1000000 uT
    When I merge mine 5 blocks via PROXY
    Then all nodes are at height 30
    Then I wait for wallet WALLET_C to have at least 1500000 uT



