# Copyright 2025 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

@offline-signing @wallet
Feature: Offline One-Sided Transaction Signing

  @critical
  Scenario: Full offline signing flow via gRPC
    # Set up infrastructure
    Given I have a seed node NODE
    When I have wallet WALLET_SENDER connected to all seed nodes
    When I have wallet WALLET_RECEIVER connected to all seed nodes
    When I have SHA3X mining node MINER connected to base node NODE and wallet WALLET_SENDER
    # Fund the sender wallet
    When mining node MINER mines 4 blocks
    Then all nodes are at height 4
    When I wait for wallet WALLET_SENDER to have at least 1002000 uT
    # Export sender keys so we can create a view-only wallet and sign offline
    Then I export wallet WALLET_SENDER view and spend keys as SENDER_KEYS
    # Create view-only wallet from exported keys
    Then I create view wallet VIEW_WALLET from view and spend keys SENDER_KEYS on node NODE
    # Wait for view wallet to discover UTXOs via chain scan
    When I wait for wallet VIEW_WALLET to have at least 1002000 uT
    # Step 1: View-only wallet prepares transaction for offline signing via gRPC
    When I prepare an offline one-sided transaction of 100000 uT from wallet VIEW_WALLET to wallet WALLET_RECEIVER at fee 20
    # Step 2: Sign the prepared transaction using the full spend wallet via its CLI
    Then I sign the prepared transaction using wallet WALLET_SENDER
    # Step 3: Broadcast the signed transaction back via the view-only wallet
    When I broadcast the signed transaction via wallet VIEW_WALLET
    # Mine blocks to confirm
    When mining node MINER mines 5 blocks
    Then all nodes are at height 9
    # Step 4: Verify receiving wallet has confirmed funds
    When I wait for wallet WALLET_RECEIVER to have at least 100000 uT

  @standalone-offline-signer
  Scenario: Full offline signing flow via standalone offline signer
    # Set up infrastructure
    Given I have a seed node NODE
    When I have wallet WALLET_SENDER connected to all seed nodes
    When I have wallet WALLET_RECEIVER connected to all seed nodes
    When I have SHA3X mining node MINER connected to base node NODE and wallet WALLET_SENDER
    # Fund the sender wallet
    When mining node MINER mines 4 blocks
    Then all nodes are at height 4
    When I wait for wallet WALLET_SENDER to have at least 1002000 uT
    # Initialize the standalone offline signer from the sender seed words, then export keys for the view-only wallet
    Then I initialize standalone offline signer OFFLINE_SIGNER from wallet WALLET_SENDER seed words
    Then I export wallet WALLET_SENDER view and spend keys as SENDER_KEYS
    # Create view-only wallet from exported keys
    Then I create view wallet VIEW_WALLET from view and spend keys SENDER_KEYS on node NODE
    # Wait for view wallet to discover UTXOs via chain scan
    When I wait for wallet VIEW_WALLET to have at least 1002000 uT
    # Step 1: View-only wallet prepares transaction for offline signing via gRPC
    When I prepare an offline one-sided transaction of 100000 uT from wallet VIEW_WALLET to wallet WALLET_RECEIVER at fee 20
    # Step 2: Sign the prepared transaction using the standalone offline signer
    Then I sign the prepared transaction using standalone offline signer OFFLINE_SIGNER
    # Step 3: Broadcast the signed transaction back via the view-only wallet
    When I broadcast the signed transaction via wallet VIEW_WALLET
    # Mine blocks to confirm
    When mining node MINER mines 5 blocks
    Then all nodes are at height 9
    # Step 4: Verify receiving wallet has confirmed funds
    When I wait for wallet WALLET_RECEIVER to have at least 100000 uT
