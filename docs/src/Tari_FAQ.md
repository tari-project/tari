# Tari FAQ
## General

### What is Tari's main website?
The website is [www.tari.com].(https://www.tari.com)

### What is the coin name? And what is it abbreviation and symbol?
Minotari. The abbreviation and symbol is XTM.

### What is the official Github repository for the Tari Project?
Tari Project's repos can be found in [https://github.com/tari-project/](https://github.com/tari-project/). The main project is [https://github.com/tari-project/tari/](https://github.com/tari-project/)

### Where can I find installation instructions for Tari?

Normal users can utilise the binaries found on Tari's main website under [www.tari.com/downloads](https://tari.com/downloads/). Developers should refer to the instructions available on in the main Tari repo at [https://github.com/tari-project/tari/](https://github.com/tari-project/)

## Technical

### What are the size requirements for the Tari chain?

On average, block size is between 1.3 and 1.5mb. This will result in the chain's average growth of around 1gb per day. Note that the maximum allowed size for a block is 4mb.

### How often is a block mined / what is the interval per block / what is the block production rate?

Blocks are mined every 120 seconds.

### What is the precision of Minotari?

Minotari can be traded at 0.000001 XTM. Fractional Minotari is referred to as Microtari (μT).

### Does Tari have a public facing API interface?

Yes. There are two access points depending on your requirements:

- Mainnet: [https://grpc.mainnet.tari.com](https://grpc.mainnet.tari.com)
- Testing: [https://grpc.nextnet.tari.com](https://grpc.nextnet.tari.com)

### Does Tari support memo functions like EOS?

No.

### Does Tari support accounts?

There are no accounts on Minotari (L1). Minotari uses a UTXO model and as such there is no specific account tied to a user's transactions on Minotari. The closest comparison is the wallet and wallet private address which is then associated with signing of transactions (and is used to validate existing UTXOs for things like calculating balance, etc).

