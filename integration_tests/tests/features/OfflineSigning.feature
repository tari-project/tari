# Copyright 2023 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause
@wallet-offline @wallet @critical
Feature: Offline Signing

  @offline-signing
  Scenario: Offline signing with view-only and full-spend wallets via gRPC
    Given I have a seed node NODE
    When I have wallet WALLET_A connected to all seed nodes
    When I have wallet WALLET_B connected to all seed nodes
    When I have SHA3X mining node MINER connected to base node NODE and wallet WALLET_A
    When mining node MINER mines 10 blocks
    Then all nodes are at height 10
    When I wait for wallet WALLET_A to have at least 10000000000 uT
    Then I export wallet WALLET_A view and spend keys as KEYS
    Then I create view wallet WALLET_C from view and spend keys KEYS on node NODE
    Then I prepare a one-sided transaction for offline signing from wallet WALLET_A to wallet WALLET_B with 1000000 uT at fee 25
    Then I sign the prepared transaction using wallet WALLET_A
    Then I broadcast the signed transaction from wallet WALLET_C
    When mining node MINER mines 5 blocks
    Then all nodes are at height 15
    Then I wait for wallet WALLET_B to have at least 1000000 uT

  @offline-signing-grpc
  Scenario: Offline signing with view-only wallet initiating via gRPC and full-spend wallet signing
    Given I have a seed node NODE
    When I have wallet SENDER connected to all seed nodes
    When I have wallet RECEIVER connected to all seed nodes
    When I have SHA3X mining node MINER connected to base node NODE and wallet SENDER
    When mining node MINER mines 10 blocks
    Then all nodes are at height 10
    When I wait for wallet SENDER to have at least 10000000000 uT
    Then I export wallet SENDER view and spend keys as SENDER_KEYS
    Then I create view wallet VIEW_ONLY_SENDER from view and spend keys SENDER_KEYS on node NODE
    Then I prepare a one-sided transaction for offline signing from wallet VIEW_ONLY_SENDER to wallet RECEIVER with 1000000 uT at fee 25
    Then I sign the prepared transaction using wallet SENDER
    Then I broadcast the signed transaction from wallet VIEW_ONLY_SENDER
    When mining node MINER mines 5 blocks
    Then all nodes are at height 15
    Then I wait for wallet RECEIVER to have at least 1000000 uT