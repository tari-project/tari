Feature: Wallet Transfer

  @long-running
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
    # We need to ensure the coinbase lock heights are gone
    When I mine 2 blocks on NODE
    When I transfer 50000 tari from Wallet_A to Wallet_B,Wallet_C at fee 100
    And I merge mine 10 blocks via PROXY
    Then all nodes are at height 27
    When I wait 300 seconds
    Then Batch transfer of 2 transactions was a success from Wallet_A to Wallet_B,Wallet_C

  Scenario: As a wallet I want to submit transfers to myself
    Given I have a seed node NODE
    And I have wallet Wallet_A connected to all seed nodes
    And I have a merge mining proxy PROXY connected to NODE and Wallet_A
    When I merge mine 2 blocks via PROXY
    Then all nodes are at height 2
    # Need to do some waiting so that wallet can check if locks are mined (approx 90 seconds)
    When I wait 90 seconds
    # Ensure the coinbase lock heights have expired
    When I mine 2 blocks on NODE
    When I transfer 50000 tari from Wallet_A to Wallet_A at fee 25
    And I merge mine 10 blocks via PROXY
    Then all nodes are at height 14
    When I wait 300 seconds
    Then Batch transfer of 1 transactions was a success from Wallet_A to Wallet_A

