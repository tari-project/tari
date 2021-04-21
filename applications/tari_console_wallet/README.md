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
$ tari_console_wallet  --command "coin-split 10000 µT 9"

1. coin-split 10000 µT 9

Coin split succeeded
Monitoring 1 sent transactions to Broadcast stage...
Done! All transactions monitored to Broadcast stage.
```

- **export-utxos**

Export all the unspent transaction outputs (UTXOs) in the wallet. This can either list the UTXOs directly in the 
console, or write them to file. In the latter case the complete unblinded set of information will be exported. 

```
tari_console_wallet --command "export-utxos"
tari_console_wallet --command "export-utxos --csv-file <file name>"
```

example output - console only:
```
$ tari_console_wallet  --command "export-utxos"

1. export-utxos

1. Value: 6000 µT OutputFeatures: Flags = (empty), Maturity = 0
2. Value: 10000 µT OutputFeatures: Flags = (empty), Maturity = 0
...
5229. Value: 5538.613962 T OutputFeatures: Flags = (empty), Maturity = 0
5230. Value: 5538.616395 T OutputFeatures: Flags = (empty), Maturity = 0
Total number of UTXOs: 5230
Total value of UTXOs: 1268921.295856 T
```

example output - `--csv-file` (console output):
```
$ tari_console_wallet  --command "export-utxos --csv-file utxos.csv"

1. export-utxos --csv-file utxos.csv

Total number of UTXOs: 11
Total value of UTXOs: 36105.165440 T
```

example output - `--csv-file` (contents of `utxos.csv`)
```
"#","Value (uT)","Spending Key","Commitment","Flags","Maturity"
"1","121999250","0b0ce2add569845ec8bb84256b731e644e2224580b568e75666399e868ea5701","22514e279bd7e7e0a6e45905e07323b16f6114e300bcc02f36b2baf44a17b43d","(empty)","0"
"2","124000000","8829254b5267de26fe926f30518604abeec156740abe64b435ac6081269f2f0d","88f4e7216353032b90bee1c8b4243c3f25b357902a5cb145fda1c98316525214","(empty)","0"
"3","125000000","295853fad02d56313920130c3a5fa3aa8be54297ee5375aa2d788f4e49f08309","72a42d2db4f8eebbc4d0074fcb04338b8caa22312ce6a68e8798c2452b52e465","(empty)","0"
...
"10","5513131148","a6323f62ef21c45f03c73b424d9823b1a0f44a4408be688e5f6fde6419a11407","dcec835619474a62b5cef8227481cebd5831aec515e85286f3932c8093e9d06b","COINBASE_OUTPUT","10655"
"11","5513145680","5af45bff0f533999c94ec799aa4789260a1b989207363c33ec6ec388899ec906","7ec353f1f005637192d50104b3c5b4621d1ebdafb5c5cc078cf3f86754669352","COINBASE_OUTPUT","10649"
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
