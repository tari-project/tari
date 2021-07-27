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

  @critical
  Scenario: Wallet imports unspent output
    Given I have a seed node NODE
    And I have 1 base nodes connected to all seed nodes
    And I have wallet WALLET_A connected to all seed nodes
    And I have a merge mining proxy PROXY connected to NODE and WALLET_A with default config
    When I merge mine 5 blocks via PROXY
    Then all nodes are at height 5
    Then I wait for wallet WALLET_A to have at least 10000000000 uT
    Then I have wallet WALLET_B connected to all seed nodes
    And I send 1000000 uT from wallet WALLET_A to wallet WALLET_B at fee 100
    When wallet WALLET_A detects all transactions are at least Broadcast
    Then I merge mine 5 blocks via PROXY
    Then all nodes are at height 10
    Then I wait for wallet WALLET_B to have at least 1000000 uT
    Then I stop wallet WALLET_B
    When I have wallet WALLET_C connected to all seed nodes
    Then I import WALLET_B unspent outputs to WALLET_C
    Then I wait for wallet WALLET_C to have at least 1000000 uT
    Then I restart wallet WALLET_C
    Then I wait for 5 seconds
    Then I wait for wallet WALLET_C to have at least 1000000 uT
    Then I check if last imported transactions are valid in wallet WALLET_C

  @critical
  Scenario: Wallet imports spent outputs that become invalidated
    Given I have a seed node NODE
    And I have 1 base nodes connected to all seed nodes
    And I have wallet WALLET_A connected to all seed nodes
    And I have a merge mining proxy PROXY connected to NODE and WALLET_A with default config
    When I merge mine 5 blocks via PROXY
    Then all nodes are at height 5
    Then I wait for wallet WALLET_A to have at least 10000000000 uT
    Then I have wallet WALLET_B connected to all seed nodes
    And I send 1000000 uT from wallet WALLET_A to wallet WALLET_B at fee 100
    When wallet WALLET_A detects all transactions are at least Broadcast
    Then I merge mine 5 blocks via PROXY
    Then all nodes are at height 10
    Then I wait for wallet WALLET_B to have at least 1000000 uT
    When I send 900000 uT from wallet WALLET_B to wallet WALLET_A at fee 100
    And wallet WALLET_B detects all transactions are at least Broadcast
    Then I merge mine 5 blocks via PROXY
    Then all nodes are at height 15
    When I wait for wallet WALLET_B to have at least 50000 uT
    Then I stop wallet WALLET_B
    When I have wallet WALLET_C connected to all seed nodes
    Then I import WALLET_B spent outputs to WALLET_C
    Then I wait for wallet WALLET_C to have at least 1000000 uT
    Then I restart wallet WALLET_C
    Then I wait for wallet WALLET_C to have less than 1 uT
    Then I check if last imported transactions are invalid in wallet WALLET_C

  @critical
  Scenario: Wallet imports reorged outputs that become invalidated
        # Chain 1
    Given I have a seed node SEED_B
    And I have a base node B connected to seed SEED_B
    And I have wallet WB connected to base node B
    And I have mining node BM connected to base node B and wallet WB
    And mining node BM mines 4 blocks with min difficulty 1 and max difficulty 50
    Then I wait for wallet WB to have at least 1000000 uT
    And I have wallet WALLET_RECEIVE_TX connected to base node B
    And I send 1000000 uT from wallet WB to wallet WALLET_RECEIVE_TX at fee 100
    And wallet WB detects all transactions are at least Broadcast
    Then mining node BM mines 4 blocks with min difficulty 50 and max difficulty 100
    When node B is at height 8
    Then I wait for wallet WALLET_RECEIVE_TX to have at least 1000000 uT
    Then I stop wallet WALLET_RECEIVE_TX
    When I have wallet WALLET_IMPORTED connected to base node B
    Then I import WALLET_RECEIVE_TX unspent outputs to WALLET_IMPORTED
        # Chain 2
    Given I have a seed node SEED_C
    And I have a base node C connected to seed SEED_C
    And I have wallet WC connected to base node C
    And I have mining node CM connected to base node C and wallet WC
    And mining node CM mines 10 blocks with min difficulty 1000 and max difficulty 9999999999
      # Connect chain 1 and 2
    Then node B is at height 8
    And node C is at height 10
    Given I have a base node SA connected to nodes B,C
    Then node SA is at height 10
    And node B is at height 10
    And node C is at height 10
    Then I restart wallet WALLET_IMPORTED
    Then I wait for wallet WALLET_IMPORTED to have less than 1 uT
    Then I check if last imported transactions are invalid in wallet WALLET_IMPORTED

Scenario: Wallet should display all transactions made
    Given I have a seed node NODE
    And I have 1 base nodes connected to all seed nodes
    And I have wallet WALLET_A connected to all seed nodes
    And I have a merge mining proxy PROXY connected to NODE and WALLET_A with default config
    When I merge mine 10 blocks via PROXY
    Then all nodes are at height 10
    Then I wait for wallet WALLET_A to have at least 10000000000 uT
    Then I have wallet WALLET_B connected to all seed nodes
    And I send 100000 uT from wallet WALLET_A to wallet WALLET_B at fee 100
    And I send 100000 uT from wallet WALLET_A to wallet WALLET_B at fee 100
    And I send 100000 uT from wallet WALLET_A to wallet WALLET_B at fee 100
    And I send 100000 uT from wallet WALLET_A to wallet WALLET_B at fee 100
    And I send 100000 uT from wallet WALLET_A to wallet WALLET_B at fee 100
    When wallet WALLET_A detects all transactions are at least Broadcast
    Then I merge mine 5 blocks via PROXY
    Then all nodes are at height 15
    Then I wait for wallet WALLET_B to have at least 500000 uT
    Then I check if wallet WALLET_B has 5 transactions
    Then I restart wallet WALLET_B
    Then I check if wallet WALLET_B has 5 transactions
