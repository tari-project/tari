@transport-selection
Feature: Transport peer selection

  Scenario Outline: Transport mode selects the expected dial protocol preference order
    Then transport type <mode> selects peer protocols <protocols>

    Examples:
      | mode    | protocols   |
      | tor     | onion       |
      | tcp     | tcp         |
      | tor_tcp | onion,tcp   |
      | tcp_tor | tcp,onion   |
