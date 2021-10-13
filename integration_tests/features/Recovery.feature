@recovery
Feature: Recovery

    Scenario Outline: Blockchain database recovery
        Given I have 2 seed nodes
        And I have a base node B connected to all seed nodes
        When I mine <NumBlocks> blocks on B
        Then all nodes are at height <NumBlocks>
        When I stop node B
        And I run blockchain recovery on node B
        And I start base node B
        Then all nodes are at height <NumBlocks>
        Examples:
            | NumBlocks |
            | 10        |

        # Takes 1min+ on Circle CI
        @long-running
        Examples:
            | NumBlocks |
            | 25        |
            | 50        |
