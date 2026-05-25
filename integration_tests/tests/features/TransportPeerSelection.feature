Feature: Transport peer selection

  Scenario Outline: transport mode selects peer protocols and dial address order
    Then transport mode "<mode>" selects peer protocols "<protocols>" and orders dial addresses "<addresses>"

    Examples:
      | mode   | protocols       | addresses       |
      | Tor    | onion           | onion           |
      | Tcp    | ipv4,ipv6       | ipv4,ipv6       |
      | TorTcp | onion,ipv4,ipv6 | onion,ipv4,ipv6 |
      | TcpTor | ipv4,ipv6,onion | ipv4,ipv6,onion |
