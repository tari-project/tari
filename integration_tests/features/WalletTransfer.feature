@wallet-transfer @wallet
Feature: Wallet Transfer

  Scenario: As a wallet I want to submit multiple transfers
    Given I have a seed node NODE
    # Add a 2nd node otherwise initial sync will not succeed
    And I have 1 base nodes connected to all seed nodes
    And I have wallet Wallet_A connected to all seed nodes
    And I have mining node MINER connected to base node NODE and wallet Wallet_A
    And I have wallet Wallet_B connected to all seed nodes
    And I have wallet Wallet_C connected to all seed nodes
    When mining node MINER mines 2 blocks
    Then all nodes are at height 2
      # Ensure the coinbase lock heights have expired
    And mining node MINER mines 3 blocks
    Then all nodes are at height 5
    # Ensure the coinbase lock heights have expired
    And mining node MINER mines 5 blocks
    Then all nodes are at height 10
    When I transfer 50000 uT from Wallet_A to Wallet_B and Wallet_C at fee 20
    And mining node MINER mines 10 blocks
    Then all nodes are at height 20
    Then all wallets detect all transactions as Mined_Confirmed

  Scenario: As a wallet I want to submit transfers to myself
    Given I have a seed node NODE
    # Add a 2nd node otherwise initial sync will not succeed
    And I have 1 base nodes connected to all seed nodes
    And I have wallet Wallet_A connected to all seed nodes
    And I have mining node MINER connected to base node NODE and wallet Wallet_A
    When mining node MINER mines 10 blocks
    Then all nodes are at height 10
    When I transfer 50000 uT to self from wallet Wallet_A at fee 25
    And I mine 5 blocks on NODE
    Then all nodes are at height 15
    Then all wallets detect all transactions as Mined_Confirmed

  Scenario: As a wallet I want to create a HTLC transaction
    Given I have a seed node NODE
    # Add a 2nd node otherwise initial sync will not succeed
    And I have 1 base nodes connected to all seed nodes
    And I have wallet WALLET_A connected to all seed nodes
    And I have wallet WALLET_B connected to all seed nodes
    And I have mining node MINER connected to base node NODE and wallet WALLET_A
    When mining node MINER mines 10 blocks
    Then I wait for wallet WALLET_A to have at least 10000000000 uT
    When I broadcast HTLC transaction with 5000000000 uT from wallet WALLET_A to wallet WALLET_B at fee 20
    And mining node MINER mines 6 blocks
    And I claim an HTLC transaction with wallet WALLET_B at fee 20
    And mining node MINER mines 6 blocks
    Then I wait for wallet WALLET_B to have at least 4000000000 uT