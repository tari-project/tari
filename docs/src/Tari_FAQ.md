# Tari FAQ
Welcome to the Tari Frequently-Asked Questions. On this page, we have provided the answers to many common questions, categorised into appropriate sections. You can check the sections below for your relevant interest:

- [General](#general) - Covers the coin name, official resources, general overview of architecture and more 
- [Tokenomics](#tokenomics) - Covers the planned economics of Tari
- [Installation and Operational Requirements](#installation-and-operational-requirements) - Covers installation and configuration of Tari, as well as operational elements for the network, such as available networks, upgrade process, public nodes lists, available networks and mode.
- [Technical Characteristics](#technical-characteristics-of-tari) - Covers the technical characteristics of Tari. Network throughput, address format, consensus model and more.
- [Development](#development) - Questions related to development and development resources.
- [Exchanges](#exchanges) - Information specific to exchanges.

## General

### What is Tari?
Tari is a decentralized, privacy-first, highly scalable and open-source blockchain protocol that aims to be infinitely scalable and provide high throughput. Tari aims to be accessible to everyone regardless of their hardware or finances.

### What is Tari Universe?
Tari Universe is a desktop application for mining Tari on Mac or PC. It works by harnessing your computer's computational power to solve blocks on the Tari network. In doing so, you're helping to secure the Tari network while earning Tari token (XTM) rewards. Tari Universe simplfies the setup of Tari while providing a beautiful interface for interacting with Tari.

### How is Tari Universe different from other apps?
Tari Universe focuses on key areas that simplify the mining process and make it accessible to a much wider audience.  
- One-click setup and installation
- A clean interface that clearly communicates information and settings.
- Automatic updates
- Built-in wallet

### What are Tari Universe's minimum and recommended requirements?
| Component     | **Minimum Desktop Spec**                             | **Recommended Desktop Spec**                                  |
|---------------|-------------------------------------------------------|----------------------------------------------------------------|
| **CPU**       | 4-core (Intel i5, Ryzen 5)                            | 6–8 core (Intel i7, Ryzen 7/9)                                 |
| **GPU**       | NVIDIA GTX 1660 or AMD RX 580 (4GB VRAM min)          | RTX 3060 / RX 6800 or better (8GB+ VRAM)                      |
| **RAM**       | 16 GB                                                 | 32 GB                           |
| **Storage**   | At least 100 GB free space, SSD                       | 250 GB or more free NVMe space SSD                                |
| **OS**        | Windows 10/11 or macOS 12+                            | Windows 11 or macOS 13+                                      |

### What are the official channels, websites, and social network profiles for Tari?
- Official Website: [www.tari.com](www.tari.com)
- Twitter: [https://twitter.com/tari/](https://twitter.com/tari/)
- Discord: [https://discord.gg/tari](https://discord.gg/tari)
- Telegram: [https://t.me/tariproject](https://t.me/tariproject)

### What are the official wallets for Minotari
Users have the three options available:

- [The Minotari Console Wallet](https://tari.com/downloads/) - Part of the Tari suite of applications. Provides a console-based wallet for transactions and interactions with the blockchain
- [Tari Universe Mobile](https://aurora.tari.com) - A mobile application for Android and iOS.
- [Tari Universe](https://airdrop.tari.com) - a desktop application that includes the entire Tari suite, including a Minotari wallet built directly into the application.

### What is the coin name, and what are its abbreviation and symbol? 
MinoTari. The abbreviation and symbol is XTM.

### Is Tari open-source?
Yes.

### What is the official Github repository for the Tari Project?
Tari Project's repos can be found in [https://github.com/tari-project/](https://github.com/tari-project/). The main project is [https://github.com/tari-project/tari/](https://github.com/tari-project/)

### Where can I find installation instructions for Tari?
+ Normal users can utilise the binaries at [https://www.tari.com/downloads]. Developers should refer to the instructions in the main Tari repo at [https://github.com/tari-project/tari/].  


### Explain the general architecture of the Tari solution? What components make up Tari?
Tari consists of several components. While it is not necessary to run all of these as a casual user, all are required if you wish to run a full base node with mining capabilities. The components are:

- Minotari Base Node (`minotari_node`): Validates and propagates Tari coin transactions and blocks on the base layer. Forms the backbone of the Tari blockchain, maintaining consensus about the longest valid proof-of-work blockchain
- Minotari Console Wallet (`minotari_console_wallet`):  Manages user funds, creates transactions, and maintains private keys. Allows users to send, receive, and manage their Tari coins. The wallet also provides a means to interact with the gRPC Wallet API. A mobile wallet is also available via [Aurora](https://aurora.tari.com)
- Minotari Miner (`minotari_miner`): Enables merge mining with Monero. Connects XMRig (Monero mining software) to the Tari network to allow simultaneous mining of both Minotari and Monero. This will be dependent on XMRig and the `minotari_merge_mining_proxy`.
- Tari Universe: A desktop-based client that handles the installation and configuration of Tari in a simple installation. Recommended for the average user.

Additional, dependent components that aren't specific to Tari are:

- XMRig: Third-party Monero mining software. Used in conjunction with the `minotari_merge_mining_proxy` to mine both Monero and Tari.
- Tor: Provides network privacy and NAT traversal. Enables secure, anonymous communication between Tari nodes.

### Are there any Tari blockchain explorers?
The mainnet explorer will be [https://explore.tari.com](https://explore.tari.com).

Please note that only Testnet explorer is currently live: https://explore-nextnet.tari.com/

## Tokenomics
### Is there any documentation available describing the tokenomics of Tari?
Users can refer to the following: [https://tari.substack.com/p/tari-tokenomics](https://tari.substack.com/p/tari-tokenomics)

### What is the tail emission of Tari?
Tari has a tail emission of 1% per year after initial supply.

### Does Tari have a token allocation plan?
There will be a total supply of 21 billion XTM emitted gradually using an exponential decay function over approximately 27.8 years. Once annual emissions decline to 1% of total supply, a perpetual 1% tail emission will continue indefinitely, ensuring ongoing miner compensation and network security. Of this total supply, 6.3 billion XTM (30%) will be pre-mined, subject to significant lockups and vesting schedules. These pre-mined tokens will support protocol infrastructure, community incentives, grants, and long-term alignment with contributors. After the pre-mine distribution, all newly emitted tokens (100%) will go exclusively to miners who secure the Tari network.

You can read more about Tari's tokenomics here: https://tari.com/tokenomics

## Installation and Operational Requirements
### What are the current networks for Tari?
- **MainNet** (`mainnet`)- The primary network for live transactions.
- **StageNet** (`stagenet`) - A stable testing network closely resembling MainNet.
- **NextNet** (`nextnet`) - A pre-production network for testing features before release.
- **TestNet** (`testnet`)- A general testing network with frequent resets and experimental features.
- **LocalNet** (`localnet`) - A local testing environment for development purposes.
- **Esmeralda** (`esmeralda`)- A stable test network used for testing and mobile wallets.

To specify which network to use when running `minotari_node`, you can set the `TARI_NETWORK` environment variable or use the `--network` command-line argument. If no network is specified, `minotari_node` will use the `testnet` by default unless overridden in the configuration file (`config.toml`) or environment variable. Ensure your configuration file (`~/.tari/config.toml` or equivalent) contains relevant settings for the selected network, such as peer seeds and port numbers.

### What are the size requirements for the Tari chain?
On average, block size is between 1.3 and 1.5mb. This will result in the chain's average growth of around 1gb per day. Note that the maximum allowed size for a block is 4mb.

### Does Tari require any rent or reservations?
No.

### Is there a whitelisting process for nodes for synchronization?
No.

### Is there a public nodes list?
There are also bootstrap nodes for the network available via TXT DNS at: [seeds.tari.com](seeds.tari.com)

The Tari protocol addresses for the bootstrap nodes are:

- ""94a4b29148621479b1c1dffb1a2f5680dcf2c0cfb901152e245d6ec299821f61::/ip4/91.134.83.67/tcp/18189""
- ""7c8e6530294e8bb7ff45b0f39e8bf8ea4e461b3df15a2df1ba3fa3a9bcc9ef71::/ip4/15.235.224.202/tcp/18189""
- ""54e491e0d88633170a98898f5d89ccc6b4f7ec22252afc23fe645d5bab453734::/ip4/198.244.203.249/tcp/18189""
- ""54e491e0d88633170a98898f5d89ccc6b4f7ec22252afc23fe645d5bab453734::/onion3/b5zgqd6emm6p2zmj7gdniysbxvmtvltrwshsyfxjoq26xqfjoicge5id:18141""
- ""94a4b29148621479b1c1dffb1a2f5680dcf2c0cfb901152e245d6ec299821f61::/onion3/x4cc7t4z5bhzz2qoj5qkfje6jnisz5gvzixeygvzwgi6sg3rhpo5rlqd:18141"" 

## Technical Characteristics of Tari
### What documentation is currently available for Tari?
- For exchanges, the best starting location is going through the exchange guide here: https://www.tari.com/lessons/09_adding_tari_to_your_exchange
- The node and wallet require grpc to interact with the API. The proto files include documentation: 
    - Node: https://github.com/tari-project/tari/blob/development/applications/minotari_app_grpc/proto/base_node.proto
    - Wallet: https://github.com/tari-project/tari/blob/development/applications/minotari_app_grpc/proto/wallet.proto
- There are some examples and more detailed explaination of the api here: https://github.com/tari-project/tari/blob/development/docs/src/API_GRPC_Explanation.md
- In addition, you can refer to the following doc for more information regarding integration with the base node via RPC: https://docs.google.com/document/d/11G2P0yEZkyJIg3WtVpBbACeqpWYgDDqQmlrUKv4HEjk/edit?tab=t.0#heading=h.ap7pxb7d730l
- The RFC Documents cover some fundamental principles around Tari, and can be located here: https://rfc.tari.com/
- Additional minor documentation can be found in: https://github.com/tari-project/tari/blob/development/docs/src/

### What kind of blockchain is Tari?
Tari is a proof-of-work, merge-mined blockchain focused on privacy and digital asset support.

### What consensus model does Tari use?
Tari uses a Proof-of-Work (PoW) model for consensus and security, and is merge-mined with Monero. Tari uses three algorithms for this purpose:
- Merge-mined RandomX: This is the same CPU-optimized algorithm used by Monero. It is merge-mined with Monero, meaning miners can mine both networks at once using the same work.
- Tari-only RandomX: Similar to the above, but it is not merge-mined, instead focused specifically on mining Tari.
- Sha3x: A custom algorithm based on SHA-3 (specifically SHA3-256), optimized for GPU mining. It's designed to provide algorithmic diversity and broaden miner participation.

The primary aim is to increase the cost of 51% attacks, ensuring mining decentralization and resilience against hardware centralization.

### Is Tari cross-chain?
No. Swaps must be performed manually at this time.

### Is Tari account-based?
No. Tari uses the UTXO model, similar to Bitcoin. As such, there is no specific account tied to a user's transactions on Minotari. The closest comparison is the private keys associated with the wallet, which are used to sign transactions and validate existing UTXOs for things like calculating balance.

### How often is a block mined / what is the interval per block / what is the block production rate?
Blocks are mined every 120 seconds.

### What is the precision of Minotari?
Minotari can be traded at 0.000001 XTM. Fractional Minotari is referred to as Microtari (μT).

### What coins is Tari similar to?
Bitcoin, Grin, Beam, Litecoin

### Does Tari have "super nodes"?
Assuming that "super nodes" in this context are high-performance, specialized nodes that play a critical role in validation, governance, or network infrastructure, then **no**. All base nodes have an equal role in ensuring the security of the network.

### What is the transaction expiration policy in the transaction pool? Is there a timeout duration after which unconfirmed transactions are removed?
Completed transactions are never removed. Pending transactions have a timeout of three (3) days; if they have not been completed within that period, the wallet will remove that transaction and mark it as canceled.

### How does Tari prevent forks?
Tari, like most blockchains, cannot definitively prevent forks, but has a number of mechanisms to minimise the chance of forks and to deal with fork occurence:

- **Proof-of-work**: A PoW model ensures blocks are valid through computational effort expended to validate the block.
- **Strongest Chain**: Tari will focus on the strongest chain (strongest being the chain with the most computational effort used to mine blocks due to higher difficulty)
- **Merge-mining with Monero**: Miners are incentivized to maintain a single, consistent chain to ensure rewards from both chains.

A list of the current strongest checkpoints are published on https://checkpoints.tari.com

### Does Tari provide dividends or interest for running a node?
No.

### Do transactions support multiple signatures?
Yes.

### Does Tari have fees? If so, what is the fee mechanism?
There is a fee. The fee for a transaction in Tari is calculated based on the transaction's weight and the specified fee-per-gram value. The relevant formula is implemented in the `Fee::calculate` method located in the `base_layer/core/src/transactions/fee.rs` file. Here's a summary of how it works:

1. Transaction Weight: The weight is determined by the number of kernels, inputs, outputs, and the size of features and scripts in bytes. This is computed using the TransactionWeight class.
2. Fee Calculation: The formula for the fee is: fee = weight * fee_per_gram

where `weight` is the total calculated weight of the transaction, and `fee_per_gram` is the fee per gram specified for the transaction.

For more details, you can explore the `Fee::calculate` implementation in the https://github.com/tari-project/tari/blob/693cd7ab56deddcd089cfbc4b4128af8e94e9786/base_layer/core/src/transactions/fee.rs.

### Does each block in the Tari blockchain include a coinbase transaction, and if so, how is the block reward distributed?
Yes, each block in the Tari blockchain includes a coinbase transaction as part of the consensus rules. The coinbase transaction is created to distribute the block reward, which consists of:

- **Block Subsidy**: Newly minted Tari as defined by the emission schedule.
- **Transaction Fees**: Fees collected from all transactions included in the block.

The entire block reward (block subsidy + transaction fees) is assigned to the miner who successfully mines the block. The coinbase transaction includes an output specifying the miner's wallet address to receive the reward. The reward structure is implemented programmatically during block construction, ensuring compliance with the consensus rules

### Does Tari support offline signing/transactions?
Yes, Tari transactions can be signed offline.

### How is the Tari blockchain secured?
Tari relies on the Nakamoto Consensus and a multi-algorithm mining approach. The hash rate is now distributed among three mining methods: approximately 33% from merged-mining Monero via RandomX, 33% from Tari-only RandomX mining, and 33% from Sha3x mining. This enhancement allows for increased mining diversity and security, supporting both merged-miners and those mining Tari directly.

### What is the block reward emission schedule in Tari?
The Tari emission schedule is designed to distribute new Tari through block rewards using an exponentially decaying model, followed by a tail emission phase. Initially, block rewards start at a high value and decay gradually over time based on a decay factor. For example, the starting block reward is approximately 12.92 Minotari (12,923,971 µT), and it decreases over time as the decay factor is applied. This approach ensures a predictable reduction in rewards, similar to Monero's emission model, without abrupt halving events.

Once the decaying rewards reach a minimum threshold, Tari introduces a tail emission phase. In this phase, miners receive a constant minimum reward per block. For instance, the tail emission reward is set to approximately 0.764 Minotari (764,000 µT) per block. This ensures ongoing miner incentives and long-term network security, preventing the total supply from becoming completely fixed. Combined with a pre-defined epoch length, the emission schedule balances early incentives with sustainable rewards over time.

The emission schedule also incorporates inflation during the tail emission phase. The inflation rate is recalculated at periodic intervals (epochs) based on the current circulating supply and is designed to remain minimal. This ensures that while the total supply continues to increase, the rate of new issuance slows significantly, maintaining economic stability within the Tari ecosystem.

### What is Tari's finality model?
While a transaction is never technically finalized in Tari, given the UTXO model, as time progresses and more blocks are mined it becomes statistically unlikely that a transaction can be reversed. We normally recommend at least 6 blocks to ensure transaction finality, which would be considered 99% finalized given the computational power required to rewrite the blockchain at that point.

### What is the transaction throughput of the Tari network?
Tari is capable of approximately 8.3 transactions per second.

### How does Tari prevent double-spending?
Tari prevents double spending through several mechanisms rooted in Mimblewimble's cryptographic principles and its enhancements with TariScript.

- **Range Proof and Commitments**:Every transaction output is represented as a cryptographic commitment, which conceals the value and the keys using Pedersen commitments. Each commitment ensures that only the private key owner can spend the output. In addition, a range proof ensures that the transaction value is positive, preventing invalid and negative values.
- **Transaction Kernel**: Every transaction in Tari includes a kernel that records the public excess value and a signature. The kernel ensures that inputs and outputs balance without revealing transaction amounts, and it prevents tampering by validating the signature.
- **Replay Prevention**: Every transaction includes the block height at the time of transaction, and is tied to the transaction. To replay the transaction, an attacker would require the private cryptographic key used to sign the transaction. Private keys are 128-bit, resulting in 2¹²⁸ possible combinations and would require, according to current mathematics and processing power, billions of years on the world's fastest computers to guess.

## Development
### Does Tari have a public facing API interface?
Yes. There are two access points depending on your requirements:

- Mainnet: [https://grpc.mainnet.tari.com](https://grpc.mainnet.tari.com)
- Testing: [https://grpc.nextnet.tari.com](https://grpc.nextnet.tari.com)

### What documentation is available for developers?

If you are interested in contributing to the Tari project, please review our community guidelines here: https://github.com/tari-project/tari/edit/development/Contributing.md

You can also review the following documents:
- The RFC Documents cover some fundamental principles around Tari, and can be located here: https://rfc.tari.com/
- Additional minor documentation can be found in: https://github.com/tari-project/tari/blob/development/docs/src/
- The node and wallet require grpc to interact with the API. The proto files include documentation: 
    - Node: https://github.com/tari-project/tari/blob/development/applications/minotari_app_grpc/proto/base_node.proto
    - Wallet: https://github.com/tari-project/tari/blob/development/applications/minotari_app_grpc/proto/wallet.proto
- There are some examples and more detailed explaination of the api here: https://github.com/tari-project/tari/blob/development/docs/src/API_GRPC_Explanation.md

You can also review the Wallet FFI here: 

### What are the rules governing the construction of addresses in Tari?
You can read more [here](https://rfc.tari.com/RFC-0155_TariAddress), but we have summarized below:

The address is a base58-encoded and adheres to the following bytes/rules:
- 0 - Network: Determines which network the address belongs to.
- 1 - Features: Indicates whether the feature set for this address is for a one-sided address or an interactive address. Valid values for these are 1 and 2 respectively.
- 2 to 33 - Public view key
- 34 to 65 - Public Spend Key
- 66 - Checksum using the DammSumm algorithm

### Does Tari support memo functions like EOS?
Yes. The memo function in this case is the Payment ID, and is encrypted with each output.

## Exchanges
### How many block confirmations are required to ensure safe transactions/credit?
For exchanges, it is recommended that a minimum of 6+ block confirmations is used to ensure safety.