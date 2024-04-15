# Copyright 2022 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

@block-template
Feature: BlockTemplate

@critical
Scenario: Verify UTXO and kernel MMR size in header
    Given I have a seed node SEED_A
    When I have 1 base nodes connected to all seed nodes
    Then meddling with block template data from node SEED_A is not allowed

    @critical
    Scenario: Verify gprc cna create block with more than 1 coinbase
        Given I have a seed node SEED_A
        When I have 1 base nodes connected to all seed nodes
        Then generate a block with 2 coinbases from node SEED_A
        Then generate a block with 2 coinbases as a single request from node SEED_A