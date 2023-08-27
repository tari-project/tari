# Copyright 2022 The Taiji Project
# SPDX-License-Identifier: BSD-3-Clause

@recovery
Feature: Recovery

    @broken
    Scenario Outline: Blockchain database recovery
        Given I have 2 seed nodes
        When I have a base node B connected to all seed nodes
        When I mine <NumBlocks> blocks on B
        Then all nodes are at height <NumBlocks>
        When I stop node B
        # block chain recovery is not working atm in base node
        # And I run blockchain recovery on node B
        # And I start base node B
        # Then all nodes are at height <NumBlocks>
        # Examples:
        #     | NumBlocks |
        #     | 10        |

        # # Takes 1min+ on Circle CI
         @long-running
         Examples:
             | NumBlocks |
             | 25        |
             | 50        |
