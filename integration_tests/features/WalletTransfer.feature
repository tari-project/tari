Feature: Wallet Transfer

  Scenario: As a wallet I want to submit multiple transfers
    Given I have a seed node NODE
    And I have wallet Wallet_A connected to all seed nodes
    And I have a merge mining proxy PROXY connected to NODE and Wallet_A
    And I have wallet Wallet_B connected to all seed nodes
    And I have wallet Wallet_C connected to all seed nodes
    When I merge mine 15 blocks via PROXY
    Then all nodes are at height 15
    # Need to do some waiting so that wallet can check if locks are mined (approx 90 seconds)
    When I wait 120 seconds
    When I send 50000 tari from Wallet_A to Wallet_B,Wallet_C at fee 100
    And I merge mine 10 blocks via PROXY
    Then all nodes are at height 25
    When I wait 300 seconds
    Then Batch transfer of 2 transactions was a success from Wallet_A to Wallet_B,Wallet_C
