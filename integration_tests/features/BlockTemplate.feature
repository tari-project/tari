# Copyright 2022 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

@block_template
Feature: BlockTemplate

Scenario: Verify UTXO and kernel MMR size in header
    Given I have a seed node SEED_A
    And I have 1 base nodes connected to all seed nodes
    Then meddling with block template data from node SEED_A is not allowed
