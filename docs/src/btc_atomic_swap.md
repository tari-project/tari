# BTC Atomic swap

This is a short document detailing how to do a Bitcoin - Taiji atomic swap

## BTC setup

The following dependencies are required:

* Taiji wallet

* Bitcoind (only for BTC atomic swaps, replace with other wallets for other coins)

* Golang

* decred atomic swap cli program


### Mac install instructions

Install Bitcoind, follow these steps: <https://github.com/bitcoin/bitcoin/blob/master/doc/build-osx.md>

Install go: <https://golang.org/doc/install>

Clone: <https://github.com/decred/atomicswap.git>

### Win install instructions

Install Bitcoind, follow these steps: <https://github.com/bitcoin/bitcoin/blob/master/doc/build-windows.md>

Install go: <https://golang.org/doc/install>

Clone: <https://github.com/decred/atomicswap.git>

### Unix install instructions

Install Bitcoind, follow these steps: <https://github.com/bitcoin/bitcoin/blob/master/doc/build-unix.md>

Install go: <https://golang.org/doc/install>

Clone: <https://github.com/decred/atomicswap.git>

### Setup instructions

Start bitcoin core with:
```cli,ignore
./src/bitcoind -daemon -testnet
```

Create a wallet with:
```cli,ignore
bitcoin-cli --testnet -named createwallet wallet_name=mywallet load_on_startup=true descriptors=false
```
`mywallet` can be any name you want to name your wallet. 
Ensure that `descriptors` is set to false. 

Create a bitcoin.conf file with an RPC username and RPC password set.
Example bitcoin.conf file: <https://github.com/bitcoin/bitcoin/blob/master/share/examples/bitcoin.conf>
Ensure this file is in the correct location: <https://github.com/bitcoin/bitcoin/blob/master/doc/bitcoin-conf.md>

Install the desired Atomic swap tool from the Decred repo with:
```cli,ignore
cd cmd/<crypto coin>
go build .
```
Replace `<crypto coin>` with the desired tool folder, for example: `btcatomicswap` for bitcoin.

### Atomic swap process

This example will use bitcoin but can be any of the other coins supported by decred atomic swap ci programs.
Alice starts the process by sending a taiji HTLC transaction with:

```cli,ignore
taiji_console_wallet --command "init-sha-atomic-swap <Amount> <Bob pubkey> <Message>"
```
`<Amount>` is the Taiji amount that is swapped
`<Bob pubkey>` is the public key used by Bob's wallet
`<Message>` is the desired Taiji transaction message

This will print out the following information:
```cli,ignore
pre_image hex: <hex>
pre_image hash: <hex>
Output hash: <hex>
```
`pre_image hex:` is the hex of the actual pre-image that is required to claim the BTC transaction.
`pre_image hash:` is the hex that Bob must use or this HTLC transaction
`Output hash:` is the hash of the output that contains the HTLC script

Alice must generate a new BTC address with:

```cli,ignore
bitcoin-cli --testnet getnewaddress "" "legacy"
```

Bob can then create the BTC HTLC transaction with:
```cli,ignore
./btcatomicswap --testnet --rpcuser=<Username> --rpcpass=<Password> participate <Alice BTC address> <BTC amount> <Pre_image hash>
```
`<Username>` This is the chosen RPC username to connect the bitcoin wallet as specified in the bitcoin.conf file
`<Password>` This is the chosen RPC password to connect the bitcoin wallet as specified in the bitcoin.conf file
`<Alice BTC address>` This is Alice's BTC address
`<BTC amount>` This is the amount of BTC Alice wants for the XTR
`<Pre_image hash>` This is the pre_image hash `pre_image hash` that Alice got when creating the XTR HTLC transaction and gave to Bob

This returns the following:
```cli,ignore
Contract fee: <fee>
Refund fee:   <fee>

Contract ():
<contract hex>

Contract transaction ():
<contract transaction hex>

Refund transaction ():
<refund transaction hex>

Publish contract transaction? [y/N]
```
Selecting `y` will publish the transaction.


After Bob publishes the transaction Alice can verify the transaction with:
```cli,ignore
./btcatomicswap --testnet --rpcuser=<Username> --rpcpass=<Password> auditcontract <contract> <contract transaction>
```
`<contract>` This is the hex of the BTC contract
`<contract transaction>`This is the hex of the BTC transaction

This will return:
```cli,ignore
Contract address:
Contract value:
Recipient address:
Author's refund address:

Secret hash:
```
Alice can verify the details and if she is happy, she can claim her BTC transaction with:

```cli,ignore
./btcatomicswap --testnet --rpcuser=<Username> --rpcpass=<Password> redeem <contract> <contract transaction> <pre_image>
```
`<contract>` This is the hex of the BTC contract
`<contract transaction>`This is the hex of the BTC transaction
`<pre_image>` This is the pre_image of the hex Alice got when she published the XTR HTLC transaction.

This will return:
```cli,ignore
Redeem fee: <fee>

Redeem transaction ():
<redeem transaction hex>

Publish redeem transaction? [y/N] 
```
Selecting y, will publish the transaction.

After Alice publishes the transaction Bob can either get the transaction hex from Alice or by calling the bitcoin cli
```cli, ignore
bitcoin-cli --testnet getrawtransaction <tx_id> false <block>
```
`<tx_id>` The redeem transaction id.
`<block>` The block hash that has the mined redeem transaction in it.

Bob can then get the pre_image by calling:
```cli,ignore
./btcatomicswap --testnet --rpcuser=<Username> --rpcpass=<Password> extractsecret <redeem tx> <Pre_image hash>
```
`<redeem tx>` the hex of the redeem transaction.
`<Pre_image hash>` the hex of the hashed pre_image

this returns:
```cli,ignore
Secret: <pre_image>
```

Bob can then claim the XTR with:
```cli,ignore
taiji_console_wallet --command "finalise-sha-atomic-swap <Output hash> <pre_image hex>"
```
`<pre_image hex:` is the hex of the actual pre-image that Bob retrieved from the BTC transaction.
`<Output hash>` is the hash of the XTR output that contains the HTLC script

### Ref guide to documentation

* Taiji atomic swap RFC: <https://rfc.taiji.com/RFC-0240_AtomicSwap.html>

* Bitcoind RPC interface: <https://developer.bitcoin.org/reference/rpc/index.html>

* Decred Atomic swap tool readme: <https://github.com/decred/atomicswap/blob/master/README.md>
