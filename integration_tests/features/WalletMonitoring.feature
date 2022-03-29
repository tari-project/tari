@wallet-monitoring  @wallet
Feature: Wallet Monitoring


  @flaky
  Scenario: Wallets monitoring coinbase after a reorg
        #
        # Chain 1:
        #   Collects 10 coinbases into one wallet
        #
    Given I have a seed node SEED_A
        # Add multiple base nodes to ensure more robust comms
    And I have a base node NODE_A1 connected to seed SEED_A
    And I have wallet WALLET_A1 connected to seed node SEED_A
    And I have mining node MINING_A connected to base node SEED_A and wallet WALLET_A1
    And mining node MINING_A mines 10 blocks
    Then all nodes are at height 10
    And I list all COINBASE transactions for wallet WALLET_A1
    Then wallet WALLET_A1 has 10 coinbase transactions
    Then all COINBASE transactions for wallet WALLET_A1 are valid
    Then wallet WALLET_A1 detects at least 7 coinbase transactions as Mined_Confirmed
        #
        # Chain 2:
        #   Collects 10 coinbases into one wallet
        #
    And I have a seed node SEED_B
        # Add multiple base nodes to ensure more robust comms
    And I have a base node NODE_B1 connected to seed SEED_B
    And I have wallet WALLET_B1 connected to seed node SEED_B
    And I have mining node MINING_B connected to base node SEED_B and wallet WALLET_B1
    And mining node MINING_B mines 10 blocks
    Then all nodes are at height 10
    And I list all COINBASE transactions for wallet WALLET_B1
    Then wallet WALLET_B1 has 10 coinbase transactions
    Then all COINBASE transactions for wallet WALLET_B1 are valid
    Then wallet WALLET_B1 detects at least 7 coinbase transactions as Mined_Confirmed
        #
        # Connect Chain 1 and 2
        #
    And I have a SHA3 miner NODE_C connected to all seed nodes
    Then all nodes are at height 10
        # When tip advances past required confirmations, invalid coinbases still being monitored will be cancelled.
    And mining node NODE_C mines 6 blocks
    Then all nodes are at height 16
        # Wait for coinbase statuses to change in the wallet
    When I wait 30 seconds
    And I list all COINBASE transactions for wallet WALLET_A1
    And I list all COINBASE transactions for wallet WALLET_B1
    Then all COINBASE transactions for wallet WALLET_A1 and wallet WALLET_B1 have consistent but opposing cancellation

  # 18+ mins on circle ci
  @long-running
  Scenario: Wallets monitoring normal transactions after a reorg
        #
        # Chain 1:
        #   Collects 10 coinbases into one wallet, send 7 transactions
        #
    And I have a seed node SEED_A
        # Add multiple base nodes to ensure more robust comms
    And I have a base node NODE_A1 connected to seed SEED_A
    And I have wallet WALLET_A1 connected to seed node SEED_A
    And I have wallet WALLET_A2 connected to seed node SEED_A
    And I have mining node MINING_A connected to base node SEED_A and wallet WALLET_A1
    When mining node MINING_A mines 10 blocks with min difficulty 20 and max difficulty 9999999999
    Then node SEED_A is at height 10
    Then node NODE_A1 is at height 10
    Then wallet WALLET_A1 detects exactly 7 coinbase transactions as Mined_Confirmed
        # Use 7 of the 10 coinbase UTXOs in transactions (others require 3 confirmations)
    And I multi-send 7 transactions of 1000000 uT from wallet WALLET_A1 to wallet WALLET_A2 at fee 100
    When mining node MINING_A mines 10 blocks with min difficulty 20 and max difficulty 9999999999
    Then node SEED_A is at height 20
    Then node NODE_A1 is at height 20
    Then wallet WALLET_A2 detects all transactions as Mined_Confirmed
    Then all NORMAL transactions for wallet WALLET_A1 are valid
    Then wallet WALLET_A1 detects exactly 17 coinbase transactions as Mined_Confirmed
        #
        # Chain 2:
        #   Collects 10 coinbases into one wallet, send 7 transactions
        #
    And I have a seed node SEED_B
        # Add multiple base nodes to ensure more robust comms
    And I have a base node NODE_B1 connected to seed SEED_B
    And I have wallet WALLET_B1 connected to seed node SEED_B
    And I have wallet WALLET_B2 connected to seed node SEED_B
    And I have mining node MINING_B connected to base node SEED_B and wallet WALLET_B1
    When mining node MINING_B mines 10 blocks with min difficulty 1 and max difficulty 2
    Then node SEED_B is at height 10
    Then node NODE_B1 is at height 10
    Then wallet WALLET_B1 detects exactly 7 coinbase transactions as Mined_Confirmed
        # Use 7 of the 10 coinbase UTXOs in transactions (others require 3 confirmations)
    And I multi-send 7 transactions of 1000000 uT from wallet WALLET_B1 to wallet WALLET_B2 at fee 100
    When mining node MINING_B mines 10 blocks with min difficulty 1 and max difficulty 2
    Then node SEED_B is at height 20
    Then node NODE_B1 is at height 20
    Then wallet WALLET_B2 detects all transactions as Mined_Confirmed
    Then all NORMAL transactions for wallet WALLET_B1 are valid
    Then wallet WALLET_B1 detects exactly 17 coinbase transactions as Mined_Confirmed
        #
        # Connect Chain 1 and 2
        #
    And I have a SHA3 miner NODE_C connected to all seed nodes
    Then all nodes are at height 20
        # When tip advances past required confirmations, invalid coinbases still being monitored will be cancelled.
    And mining node NODE_C mines 6 blocks
    Then all nodes are at height 26
    Then wallet WALLET_A1 detects exactly 20 coinbase transactions as Mined_Confirmed
    Then wallet WALLET_B1 detects exactly 17 coinbase transactions as Mined_Confirmed
    And I list all NORMAL transactions for wallet WALLET_A1
    And I list all NORMAL transactions for wallet WALLET_B1
    # TODO: Uncomment this step when wallets can handle reorg
#    Then all NORMAL transactions for wallet WALLET_A1 and wallet WALLET_B1 have consistent but opposing cancellation
    And I list all NORMAL transactions for wallet WALLET_A2
    And I list all NORMAL transactions for wallet WALLET_B2
    # TODO: Uncomment this step when wallets can handle reorg
#    Then all NORMAL transactions for wallet WALLET_A2 and wallet WALLET_B2 have consistent but opposing cancellation
    When I wait 1 seconds

  Scenario Outline: Verify all coinbases in hybrid mining are accounted for
    Given I have a seed node SEED_A
    And I have a SHA3 miner MINER_SEED_A connected to seed node SEED_A

    And I have a base node NODE1 connected to seed SEED_A
    And I have wallet WALLET1 connected to base node NODE1
    And I have a merge mining proxy PROXY1 connected to NODE1 and WALLET1 with default config

    And I have a base node NODE2 connected to seed SEED_A
    And I have wallet WALLET2 connected to base node NODE2
    And I have mining node MINER2 connected to base node NODE2 and wallet WALLET2

    When I co-mine <numBlocks> blocks via merge mining proxy PROXY1 and mining node MINER2
    Then all nodes are on the same chain tip

    And mining node MINER_SEED_A mines 5 blocks
    Then all nodes are on the same chain tip

    When I wait 1 seconds
    Then wallets WALLET1,WALLET2 should have AT_LEAST <numBlocks> spendable coinbase outputs

    @flaky
    Examples:
      | numBlocks |
      | 10        |

    @long-running @flaky
    Examples:
        | numBlocks |
        | 100       |
        | 1000      |
        | 4500      |
