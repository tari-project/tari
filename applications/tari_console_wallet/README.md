# Tari Console Wallet

The Tari Console Wallet is a terminal based wallet for sending and receiving Tari. It can be run in a few different modes.

### Terminal UI (TUI) mode
The standard operating mode, TUI mode is the default when starting `tari_console_wallet`. Displays a UI in the terminal to interact with the wallet.

![](./docs/img/tui.png)

### Daemon (GRPC) mode
Run as a server with no UI, but exposing the GRPC interface with `tari_console_wallet --daemon`.

### Command mode
Run a once off command with the `--command` argument:

- **get-balance**

Get your wallet balance

`tari_console_wallet --command "get-balance"`

example output:
```
Available balance: 1268922.299856 T
Pending incoming balance: 6010 µT
Pending outgoing balance: 1.337750 T
```

- **send-tari**

Send an amount of Tari to a public key or emoji id.

`tari_console_wallet --command "send-tari <amount> <pubkey> <optional message>"`

example:
```
$ tari_console_wallet --command "send-tari 1T c69fbe5f05a304eaec65d5f234a6aa258a90b8bb5b9ceffea779653667ef2108 coffee"

1. send-tari 1.000000 T c69fbe5f05a304eaec65d5f234a6aa258a90b8bb5b9ceffea779653667ef2108 coffee
Monitoring 1 sent transactions to Broadcast stage...
Done! All transactions monitored to Broadcast stage.
```

- **make-it-rain**

Make it rain! Send many transactions to a public key or emoji id.

`tari_console_wallet --command "make-it-rain <tx/sec> <duration> <amount> <increment> <start time or now> <pubkey> <optional message>"`

example:
```
$ tari_console_wallet  --command "make-it-rain 1 10 8000 100 now c69fbe5f05a304eaec65d5f234a6aa258a90b8bb5b9ceffea779653667ef2108 makin it rain yo"

1. make-it-rain 1 10 8000 µT 100 µT 2021-03-26 10:03:30.459157 UTC c69fbe5f05a304eaec65d5f234a6aa258a90b8bb5b9ceffea779653667ef2108 makin it rain yo
Monitoring 10 sent transactions to Broadcast stage...
Done! All transactions monitored to Broadcast stage.
```

- **coin-split**

Split one or more unspent transaction outputs into many.
Creates a transaction that must be mined before the new outputs can be spent.

`tari_console_wallet --command "coin-split <amount per coin> <number of coins>"`
example:
$ tari_console_wallet --command "coin-split 10000 9"
example output:
```
1. coin-split 10000 µT 9
Coin split succeeded
Monitoring 1 sent transactions to Broadcast stage...
Done! All transactions monitored to Broadcast stage.
```

- **list-utxos**

List all of the unspent transaction outputs (UTXOs) in the wallet.

`tari_console_wallet --command "list-utxos"`

example output:
```
1. list-utxos
1. Value: 6000 µT OutputFeatures: Flags = (empty), Maturity = 0
2. Value: 10000 µT OutputFeatures: Flags = (empty), Maturity = 0
...
5229. Value: 5538.613962 T OutputFeatures: Flags = (empty), Maturity = 0
5230. Value: 5538.616395 T OutputFeatures: Flags = (empty), Maturity = 0
Total number of UTXOs: 5230
Total value of UTXOs: 1268921.295856 T
```

- **count-utxos**

Count the number of unspent transaction outputs (UTXOs) in the wallet.

`tari_console_wallet --command "count-utxos"`

example output:
```
1. count-utxos
Total number of UTXOs: 5230
Total value of UTXOs : 1268921.295856 T
Minimum value UTXO   : 6000 µT
Average value UTXO   : 242.623575 T
Maximum value UTXO   : 5538.616395 T
```

- **discover-peer**

Discover a peer on the network by public key or emoji id.

`tari_console_wallet --command "discover-peer <public key or emoji id>"`

example output:
```
1. discover-peer c69fbe5f05a304eaec65d5f234a6aa258a90b8bb5b9ceffea779653667ef2108
Waiting for connectivity... ✅
🌎 Peer discovery started.
⚡️ Discovery succeeded in 16420ms.
[dbb4bfde6a67a8e0] PK=c69fbe5f05a304eaec65d5f234a6aa258a90b8bb5b9ceffea779653667ef2108 (/onion3/zs2wpll7zdvxunfnxyhkhan4ntjsps72zutfybssnobvpff63pg6j4qd:18101) - . Type: WALLET. User agent: tari/wallet/0.8.5. Last connected at 2021-03-26 09:07:15.
```

- **whois**

Look up a public key or emoji id, useful for converting between the two formats.

`tari_console_wallet --command "whois <public key or emoji id>"`

example output:
```
1. whois c69fbe5f05a304eaec65d5f234a6aa258a90b8bb5b9ceffea779653667ef2108
Public Key: c69fbe5f05a304eaec65d5f234a6aa258a90b8bb5b9ceffea779653667ef2108
Emoji ID  : 📈👛💭🎾🌍👡🌋😻🚀🏉🔥🚓🍳👹👿🍕🐵🐼💡💦🎺👘🚌🚿👻🐛🏉🍵🏥🚌🍑🌞🍹
```

### Script mode

Run a series of commands from a given script.

`tari_console_wallet --script /path/to/script`

### Recovery mode
todo docs
