# Copyright 2022. The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

@offline-signing @critical
Feature: Offline Signing gRPC Flow

  Verify the complete offline signing workflow via gRPC without MoneroD dependency

  @smoke
  Scenario: Basic offline signing flow
    Given a view-only wallet "test_view_wallet" is created
    When I initiate offline signing via gRPC for wallet "test_view_wallet"
    Then the offline signing process completes successfully
    When a signed transaction is broadcast via view-only wallet "test_view_wallet"
    Then the receiving wallet confirms the transaction

  @detailed
  Scenario: Offline signing with multiple inputs and outputs
    Given a view-only wallet "complex_view_wallet" is created
    When I initiate offline signing via gRPC for wallet "complex_view_wallet"
    Then the offline signing process completes successfully
    When a signed transaction is broadcast via view-only wallet "complex_view_wallet"
    Then the receiving wallet confirms the transaction
    And the transaction has multiple inputs and outputs
