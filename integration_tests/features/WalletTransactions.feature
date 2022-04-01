# Copyright 2022 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

@wallet-transact @wallet
Feature: Wallet Transactions

  Scenario: Wallet sending and receiving one-sided transactions
    Given I have a seed node NODE
    And I have 1 base nodes connected to all seed nodes
    And I have wallet WALLET_A connected to all seed nodes
    And I have wallet WALLET_B connected to all seed nodes
    And I have wallet WALLET_C connected to all seed nodes
    And I have mining node MINER connected to base node NODE and wallet WALLET_A
    When mining node MINER mines 15 blocks
    Then all nodes are at height 15
    When I wait for wallet WALLET_A to have at least 55000000000 uT
    Then I send a one-sided transaction of 1000000 uT from WALLET_A to WALLET_B at fee 100
    Then I send a one-sided transaction of 1000000 uT from WALLET_A to WALLET_B at fee 100
    When mining node MINER mines 5 blocks
    Then all nodes are at height 20
    Then I wait for wallet WALLET_B to have at least 2000000 uT
    # Spend one of the recovered UTXOs to self in a standard MW transaction
    Then I send 900000 uT from wallet WALLET_B to wallet WALLET_B at fee 20
    Then I wait for wallet WALLET_B to have less than 1100000 uT
    When mining node MINER mines 5 blocks
    Then all nodes are at height 25
    Then I wait for wallet WALLET_B to have at least 1900000 uT
    # Make a one-sided payment to a new wallet that is big enough to ensure the second recovered output is spent
    Then I send a one-sided transaction of 1500000 uT from WALLET_B to WALLET_C at fee 20
    Then I wait for wallet WALLET_B to have less than 1000000 uT
    When mining node MINER mines 5 blocks
    Then all nodes are at height 30
    Then I wait for wallet WALLET_C to have at least 1500000 uT

  Scenario: Wallet imports unspent output
    Given I have a seed node NODE
    And I have 1 base nodes connected to all seed nodes
    And I have wallet WALLET_A connected to all seed nodes
    And I have wallet WALLET_B connected to all seed nodes
    And I have wallet WALLET_C connected to all seed nodes
    And I have mining node MINER connected to base node NODE and wallet WALLET_A
    When mining node MINER mines 5 blocks
    Then all nodes are at height 5
    Then I wait for wallet WALLET_A to have at least 10000000000 uT
    And I send 1000000 uT from wallet WALLET_A to wallet WALLET_B at fee 100
    Then mining node MINER mines 5 blocks
    Then all nodes are at height 10
    Then I wait for wallet WALLET_B to have at least 1000000 uT
    Then I stop wallet WALLET_B
    Then I import WALLET_B unspent outputs to WALLET_C
    Then I wait for wallet WALLET_C to have at least 1000000 uT
    Then I restart wallet WALLET_C
    Then I wait for wallet WALLET_C to have at least 1000000 uT
    Then I check if last imported transactions are valid in wallet WALLET_C

  @flaky
  Scenario: Wallet imports spent outputs that become invalidated
    Given I have a seed node NODE
    And I have 1 base nodes connected to all seed nodes
    And I have wallet WALLET_A connected to all seed nodes
    And I have wallet WALLET_B connected to all seed nodes
    And I have wallet WALLET_C connected to all seed nodes
    And I have mining node MINER connected to base node NODE and wallet WALLET_A
    When mining node MINER mines 5 blocks
    Then all nodes are at height 5
    Then I wait for wallet WALLET_A to have at least 10000000000 uT
    And I send 1000000 uT from wallet WALLET_A to wallet WALLET_B at fee 100
    Then mining node MINER mines 5 blocks
    Then all nodes are at height 10
    Then I wait for wallet WALLET_B to have at least 1000000 uT
    When I send 900000 uT from wallet WALLET_B to wallet WALLET_A at fee 100
    Then mining node MINER mines 5 blocks
    Then all nodes are at height 15
    When I wait for wallet WALLET_B to have at least 50000 uT
    Then I stop wallet WALLET_B
    Then I import WALLET_B spent outputs to WALLET_C
    Then I wait for wallet WALLET_C to have at least 1000000 uT
    Then I restart wallet WALLET_C
    Then I wait for wallet WALLET_C to have less than 1 uT
  # TODO Either remove the check for invalid Faux tx and change the test name or implement a new way to invalidate Faux Tx
  # The concept of invalidating the Faux transaction doesn't exist in this branch anymore. There has been talk of removing the Faux transaction
  # for imported UTXO's anyway so until that is decided we will just check that the imported output becomes Spent
  #Then I check if last imported transactions are invalid in wallet WALLET_C

  @flaky
  Scenario: Wallet imports reorged outputs that become invalidated
    # Chain 1
    Given I have a seed node SEED_B
    And I have a base node B connected to seed SEED_B
    And I have wallet WB connected to base node B
    And I have wallet WALLET_RECEIVE_TX connected to base node B
    And I have wallet WALLET_IMPORTED connected to base node B
    And I have mining node BM connected to base node B and wallet WB
    And mining node BM mines 4 blocks with min difficulty 1 and max difficulty 50
    Then I wait for wallet WB to have at least 1000000 uT
    And I send 1000000 uT from wallet WB to wallet WALLET_RECEIVE_TX at fee 100
    Then mining node BM mines 4 blocks with min difficulty 50 and max difficulty 100
    When node B is at height 8
    Then I wait for wallet WALLET_RECEIVE_TX to have at least 1000000 uT
    Then I stop wallet WALLET_RECEIVE_TX
    Then I import WALLET_RECEIVE_TX unspent outputs to WALLET_IMPORTED
    Then I wait for wallet WALLET_IMPORTED to have at least 1000000 uT
    # This triggers a validation of the imported outputs
    Then I restart wallet WALLET_IMPORTED
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
    And mining node CM mines 1 blocks with min difficulty 1000 and max difficulty 9999999999
    And node B is at height 11
    And node C is at height 11
  # TODO Either remove the check for invalid Faux tx and change the test name or implement a new way to invalidate Faux Tx
  # The concept of invalidating the Faux transaction doesn't exist in this branch anymore. There has been talk of removing the Faux transaction
  # for imported UTXO's anyway so until that is decided we will just check that the imported output becomes invalid
  # Then I check if last imported transactions are invalid in wallet WALLET_IMPORTED

  Scenario: Wallet imports faucet UTXO
    Given I have a seed node NODE
    And I have 1 base nodes connected to all seed nodes
    And I have wallet WALLET_A connected to all seed nodes
    And I have wallet WALLET_B connected to all seed nodes
    And I have wallet WALLET_C connected to all seed nodes
    And I have mining node MINER connected to base node NODE and wallet WALLET_A
    When mining node MINER mines 5 blocks
    Then all nodes are at height 5
    Then I wait for wallet WALLET_A to have at least 10000000000 uT
    And I send 1000000 uT from wallet WALLET_A to wallet WALLET_B at fee 100
    When mining node MINER mines 6 blocks
    Then all nodes are at height 11
    Then I wait for wallet WALLET_B to have at least 1000000 uT
    Then I stop wallet WALLET_B
    Then I import WALLET_B unspent outputs as faucet outputs to WALLET_C
    Then I wait for wallet WALLET_C to have at least 1000000 uT
    And I send 500000 uT from wallet WALLET_C to wallet WALLET_A at fee 100
    When mining node MINER mines 6 blocks
    Then all nodes are at height 17
    Then I wait for wallet WALLET_C to have at least 400000 uT

  Scenario: Wallet should display all transactions made
    Given I have a seed node NODE
    And I have 1 base nodes connected to all seed nodes
    And I have wallet WALLET_A connected to all seed nodes
    And I have wallet WALLET_B connected to all seed nodes
    And I have mining node MINER connected to base node NODE and wallet WALLET_A
    When mining node MINER mines 10 blocks
    Then all nodes are at height 10
    Then I wait for wallet WALLET_A to have at least 10000000000 uT
    And I send 100000 uT from wallet WALLET_A to wallet WALLET_B at fee 100
    And I send 100000 uT from wallet WALLET_A to wallet WALLET_B at fee 100
    And I send 100000 uT from wallet WALLET_A to wallet WALLET_B at fee 100
    And I send 100000 uT from wallet WALLET_A to wallet WALLET_B at fee 100
    And I send 100000 uT from wallet WALLET_A to wallet WALLET_B at fee 100
    Then mining node MINER mines 5 blocks
    Then all nodes are at height 15
    Then I wait for wallet WALLET_B to have at least 500000 uT
    Then I check if wallet WALLET_B has 5 transactions
    Then I restart wallet WALLET_B
    Then I check if wallet WALLET_B has 5 transactions

  Scenario: Wallet clearing out invalid transactions after a reorg
    #
    # Chain 1:
    #   Collects 7 coinbases into one wallet, send 7 transactions
    #   Stronger chain
    #
    Given I have a seed node SEED_A
    And I have a base node NODE_A1 connected to seed SEED_A
    And I have wallet WALLET_A1 connected to seed node SEED_A
    And I have wallet WALLET_A2 connected to seed node SEED_A
    And I have mining node MINER_A1 connected to base node SEED_A and wallet WALLET_A1
    When mining node MINER_A1 mines 7 blocks with min difficulty 200 and max difficulty 100000
    Then node SEED_A is at height 7
    Then node NODE_A1 is at height 7
    When I mine 3 blocks on SEED_A
    Then wallet WALLET_A1 detects at least 7 coinbase transactions as Mined_Confirmed
    Then node SEED_A is at height 10
    Then node NODE_A1 is at height 10
    And I multi-send 7 transactions of 1000000 uT from wallet WALLET_A1 to wallet WALLET_A2 at fee 100
    #
    # Chain 2:
    #   Collects 7 coinbases into one wallet, send 7 transactions
    #   Weaker chain
    #
    And I have a seed node SEED_B
    And I have a base node NODE_B1 connected to seed SEED_B
    And I have wallet WALLET_B1 connected to seed node SEED_B
    And I have wallet WALLET_B2 connected to seed node SEED_B
    And I have mining node MINER_B1 connected to base node SEED_B and wallet WALLET_B1
    When mining node MINER_B1 mines 7 blocks with min difficulty 1 and max difficulty 100
    Then node SEED_B is at height 7
    Then node NODE_B1 is at height 7
    When I mine 5 blocks on SEED_B
    Then wallet WALLET_B1 detects at least 7 coinbase transactions as Mined_Confirmed
    Then node SEED_B is at height 12
    Then node NODE_B1 is at height 12
    And I multi-send 7 transactions of 1000000 uT from wallet WALLET_B1 to wallet WALLET_B2 at fee 100
    #
    # Connect Chain 1 and 2 in stages
    #    New node connects to weaker chain, receives all broadcast (not mined) transactions into mempool
    #    New node connects to stronger chain, then reorgs its complete chain
    #    New node mines blocks; no invalid inputs from the weaker chain should be used in the block template
    #
    And I have a base node NODE_C connected to seed SEED_B
    Then node NODE_C is at height 12
    # Wait for the reorg to filter through
    And I connect node SEED_A to node NODE_C
    Then all nodes are at height 10
    When I mine 6 blocks on NODE_C
    Then all nodes are at height 16

  Scenario: Wallet send transactions while offline
    Given I have a seed node SEED
    And I have wallet WALLET_A connected to seed node SEED
    And I have wallet WALLET_B connected to seed node SEED
    And I have mining node MINER_A connected to base node SEED and wallet WALLET_A
    When mining node MINER_A mines 1 blocks with min difficulty 1 and max difficulty 100000
    When I mine 4 blocks on SEED
    Then I wait for wallet WALLET_A to have at least 1000000000 uT
    When I stop wallet WALLET_B
    When I stop node SEED
    Then I wait 10 seconds
    Then I send 100000000 uT without waiting for broadcast from wallet WALLET_A to wallet WALLET_B at fee 20
    Then I wait 10 seconds
    And I start base node SEED
    And I have a base node NODE_A connected to seed SEED
    And I have a base node NODE_B connected to seed SEED
    And I stop wallet WALLET_A
    And I start wallet WALLET_A
    And I start wallet WALLET_B
    Then all nodes are at height 5
    When I mine 1 blocks on SEED
    Then all nodes are at height 6
    Then wallet WALLET_B detects all transactions are at least Pending
    Then I wait 1 seconds

  Scenario: Short wallet clearing out invalid transactions after a reorg
    #
    # Chain 1:
    #   Collects 7 coinbases into one wallet, send 7 transactions
    #   Stronger chain
    #
    Given I have a seed node SEED_A
    And I have a base node NODE_A1 connected to seed SEED_A
    And I have wallet WALLET_A1 connected to seed node SEED_A
    And I have wallet WALLET_A2 connected to seed node SEED_A
    And I have mining node MINER_A1 connected to base node SEED_A and wallet WALLET_A1
    When mining node MINER_A1 mines 1 blocks with min difficulty 200 and max difficulty 100000
    Then node SEED_A is at height 1
    Then node NODE_A1 is at height 1
    When I mine 3 blocks on SEED_A
    Then wallet WALLET_A1 detects at least 1 coinbase transactions as Mined_Confirmed
    Then node SEED_A is at height 4
    Then node NODE_A1 is at height 4
    And I multi-send 1 transactions of 10000 uT from wallet WALLET_A1 to wallet WALLET_A2 at fee 20
    #
    # Chain 2:
    #   Collects 7 coinbases into one wallet, send 7 transactions
    #   Weaker chain
    #
    And I have a seed node SEED_B
    And I have a base node NODE_B1 connected to seed SEED_B
    And I have wallet WALLET_B1 connected to seed node SEED_B
    And I have wallet WALLET_B2 connected to seed node SEED_B
    And I have mining node MINER_B1 connected to base node SEED_B and wallet WALLET_B1
    When mining node MINER_B1 mines 2 blocks with min difficulty 1 and max difficulty 100
    Then node SEED_B is at height 2
    Then node NODE_B1 is at height 2
    When I mine 3 blocks on SEED_B
    Then wallet WALLET_B1 detects at least 2 coinbase transactions as Mined_Confirmed
    Then node SEED_B is at height 5
    Then node NODE_B1 is at height 5
    And I multi-send 2 transactions of 10000 uT from wallet WALLET_B1 to wallet WALLET_B2 at fee 20
    #
    # Connect Chain 1 and 2 in stages
    #    New node connects to weaker chain, receives all broadcast (not mined) transactions into mempool
    #    New node connects to stronger chain, then reorgs its complete chain
    #    New node mines blocks; no invalid inputs from the weaker chain should be used in the block template
    #
    And I have a base node NODE_C connected to seed SEED_B
    Then node NODE_C is at height 5
    # Wait for the reorg to filter through
    And I connect node SEED_A to node NODE_C
    Then all nodes are at height 4
    When I mine 2 blocks on NODE_C
    Then all nodes are at height 6

  @flaky @long-running
  Scenario: Wallet SAF negotiation and cancellation with offline peers
    Given I have a seed node NODE
    And I have 1 base nodes connected to all seed nodes
    And I have wallet WALLET_A connected to all seed nodes
    And I have wallet WALLET_RECV connected to all seed nodes
    And I have mining node MINER connected to base node NODE and wallet WALLET_A
    And mining node MINER mines 5 blocks
    Then all nodes are at height 5
    Then I wait for wallet WALLET_A to have at least 10000000000 uT
    And I have non-default wallet WALLET_SENDER connected to all seed nodes using StoreAndForwardOnly
    And I send 100000000 uT from wallet WALLET_A to wallet WALLET_SENDER at fee 100
    And mining node MINER mines 5 blocks
    Then all nodes are at height 10
    Then I wait for wallet WALLET_SENDER to have at least 100000000 uT
    And I stop wallet WALLET_RECV
    And I send 1000000 uT without waiting for broadcast from wallet WALLET_SENDER to wallet WALLET_RECV at fee 100
    When wallet WALLET_SENDER detects last transaction is Pending
    Then I stop wallet WALLET_SENDER
    And I start wallet WALLET_RECV
    And I wait 5 seconds
    When wallet WALLET_RECV detects all transactions are at least Pending
    Then I cancel last transaction in wallet WALLET_RECV
    When I wait 15 seconds
    Then I stop wallet WALLET_RECV
    And I start wallet WALLET_SENDER
    # This is a weirdness that I haven't been able to figure out. When you start WALLET_SENDER on the line above it
    # requests SAF messages from the base nodes the base nodes get the request and attempt to send the stored messages
    # but the connection fails. It requires a second reconnection and request for the SAF messages to be delivered.
    And I wait 10 seconds
    Then I restart wallet WALLET_SENDER
    And I wait 10 seconds
    Then I restart wallet WALLET_SENDER
    When I wait 30 seconds
    And mining node MINER mines 5 blocks
    Then all nodes are at height 15
    When wallet WALLET_SENDER detects all transactions as Mined_Confirmed
    And I start wallet WALLET_RECV
    And I wait 5 seconds
    Then I restart wallet WALLET_RECV
    And I wait 5 seconds
    Then I restart wallet WALLET_RECV
    Then I wait for wallet WALLET_RECV to have at least 1000000 uT
