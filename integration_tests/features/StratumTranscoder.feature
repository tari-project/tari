@stratum-transcoder
Feature: Stratum Transcoder

  @flaky @broken
  Scenario: Transcoder Functionality Test
    Given I have a seed node NODE
    And I have wallet WALLET1 connected to all seed nodes
    And I have wallet WALLET2 connected to all seed nodes
    And I have wallet WALLET3 connected to all seed nodes
    And I have wallet WALLET4 connected to all seed nodes
    # For funds
    And I have a merge mining proxy FUNDS connected to NODE and WALLET1 with default config
    When I merge mine 10 blocks via FUNDS
    # end
    # Transactions sent by proxy are one-sided, only the sending wallet needs to be online
    And I stop wallet WALLET2
    And I stop wallet WALLET3
    And I stop wallet WALLET4
    And I have a stratum transcoder PROXY connected to NODE and WALLET1
    When I call getinfo from stratum transcoder PROXY
    Then I get a valid getinfo response from stratum transcoder PROXY
    # Ordering is important here
    When I call getblocktemplate from stratum transcoder PROXY
    Then I get a valid getblocktemplate response from stratum transcoder PROXY
    When I call submitblock from stratum transcoder PROXY
    Then I get a valid submitblock response from stratum transcoder PROXY
    When I call getlastblockheader from stratum transcoder PROXY
    Then I get a valid getlastblockheader response from stratum transcoder PROXY
    When I call getblockheaderbyheight from stratum transcoder PROXY
    Then I get a valid getblockheaderbyheight response from stratum transcoder PROXY
    When I call getblockheaderbyhash from stratum transcoder PROXY
    Then I get a valid getblockheaderbyhash response from stratum transcoder PROXY
    # end
    Then I wait for wallet WALLET1 to have at least 5000 uT
    When I call getbalance from stratum transcoder PROXY
    Then I get a valid getbalance response from stratum transcoder PROXY
    When I call transfer from stratum transcoder PROXY using the public key of WALLET2, WALLET3 and amount 1000 uT each
    # Flaky here, sometimes the transaction fails on CI and is_success is returned as false.
    # While this is correct behaviour on the part of the transcoder where we are expecting
    # the transaction to succeed to be able to check balances later in the test.
    # Will need to check logs in base node and wallet to determine why the transaction failed.
    Then I get a valid transfer response from stratum transcoder PROXY
    # For mined transactions
    When I merge mine 5 blocks via FUNDS
    When I call transfer from stratum transcoder PROXY using the public key of WALLET3, WALLET4 and amount 1000 uT each
    Then I get a valid transfer response from stratum transcoder PROXY
    # For mined transactions
    When I merge mine 5 blocks via FUNDS
    When I call transfer from stratum transcoder PROXY using the public key of WALLET1, WALLET4 and amount 1000 uT each
    Then I get a valid transfer response from stratum transcoder PROXY
    # For mined transactions
    When I merge mine 5 blocks via FUNDS
    When I call transfer from stratum transcoder PROXY using the public key of WALLET2, WALLET4 and amount 1000 uT each
    Then I get a valid transfer response from stratum transcoder PROXY
    # For mined transactions
    When I merge mine 5 blocks via FUNDS
    Then node NODE is at height 31
    Then I start wallet WALLET2
    Then I start wallet WALLET3
    Then I start wallet WALLET4
    Then I wait for wallet WALLET2 to have at least 2000 uT
    Then I wait for wallet WALLET3 to have at least 2000 uT
    Then I wait for wallet WALLET4 to have at least 2000 uT
