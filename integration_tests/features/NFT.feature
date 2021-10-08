@dan
Feature: NFT
    Scenario: Minting tokens
        Given I have a seed node SEED
        And I have wallet WALLET_A connected to seed node SEED
        And I have mining node MINER connected to seed node SEED and wallet WALLET_A
        And mining node MINER mines 5 blocks
        Then I wait for wallet WALLET_A to have at least 1000000 uT
        And I wait 30 seconds
        And I register asset FACTORY on wallet WALLET_A
        Then I have asset FACTORY on wallet WALLET_A with status ENCUMBEREDTOBERECEIVED
        And mining node MINER mines 1 block
        # TODO: remove recovery and do the test on just one wallet
        When I recover wallet WALLET_A into wallet WALLET_B connected to all seed nodes
        Then I have asset FACTORY on wallet WALLET_B with status UNSPENT
        And I mint tokens "TOKEN1 TOKEN2" for asset FACTORY on wallet WALLET_B
        Then I have token TOKEN1 for asset FACTORY on wallet WALLET_B in state ENCUMBEREDTOBERECEIVED
        And mining node MINER mines 1 block
        # TODO: remove recovery and do the test on just one wallet
        When I recover wallet WALLET_B into wallet WALLET_C connected to all seed nodes
        Then I have token TOKEN1 for asset FACTORY on wallet WALLET_C in state UNSPENT
        # TODO: If the following lines fail, we can rewrite the both tests to single wallet
        Then I have token TOKEN1 for asset FACTORY on wallet WALLET_B in state ENCUMBEREDTOBERECEIVED
        Then I have asset FACTORY on wallet WALLET_A with status ENCUMBEREDTOBERECEIVED

    Scenario: Minting tokens via command line
        Given I have a seed node SEED
        And I have wallet WALLET_A connected to seed node SEED
        And I have mining node MINER connected to seed node SEED and wallet WALLET_A
        And mining node MINER mines 5 blocks
        Then I wait for wallet WALLET_A to have at least 1000000 uT
        And I wait 30 seconds
        And I register asset FACTORY on wallet WALLET_A via command line
        Then I have asset FACTORY on wallet WALLET_A with status ENCUMBEREDTOBERECEIVED
        And mining node MINER mines 1 block
        # TODO: remove recovery and do the test on just one wallet
        When I recover wallet WALLET_A into wallet WALLET_B connected to all seed nodes
        Then I have asset FACTORY on wallet WALLET_B with status UNSPENT
        And I mint tokens "TOKEN1 TOKEN2" for asset FACTORY on wallet WALLET_B via command line
        Then I have token TOKEN1 for asset FACTORY on wallet WALLET_B in state ENCUMBEREDTOBERECEIVED
        And mining node MINER mines 1 block
        # TODO: remove recovery and do the test on just one wallet
        When I recover wallet WALLET_B into wallet WALLET_C connected to all seed nodes
        Then I have token TOKEN1 for asset FACTORY on wallet WALLET_C in state UNSPENT
