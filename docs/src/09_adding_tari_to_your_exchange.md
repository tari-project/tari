---
layout: lesson
title: Adding Tari to Your Exchange
date: 2024-11-06 12:00
author: stringhandler
subtitle:
class: subpage
---
In this guide, we will cover the basics of getting a Minotari node operational and explain the process of setting up the associated wallets required to run Minotari on your exchange securely:

* We will set up your own Minotari node
* We will cover the creation of a Minotari wallet as the store of funds, as well as a corresponding read-only wallet for use by the exchange for monitoring transactions
* We will discuss how to monitor the blockchain for transactions
* We will cover both the depositing and withdrawal of funds to the wallet

This guide assumes that the node will not be used for mining.

## Node Setup
In order to accept Tari, you will need to have a Minotari node. While it is possible to use a public node with the correct gRPC methods exposed to the internet, it is recommended that you run your own node. 

It may also be worth running multiple nodes as backups to ensure availability.

> Note: For all servers connected to the internet, they must either be running a Tor client or configure their public IP information. Documentation on this is available [here](https://github.com/tari-project/tari#README) and [here](https://github.com/tari-project/tari/discussions/6366). If you are running on Linux, the Tari applications have built-in Tor support, so this can be ignored.

### Section 1: Create a Minotari Node and run it
The Minotari node is the base layer node required to receive and monitor transcations. 

> NOTE: If you are using a public Minotari node, you can skip this section. Note that you will need the `public key` and the `public address` of the public node in question in order to correctly proceed with this exchange guide.

1. Download the compiled binaries [here](https://tari.com/downloads/). If you would prefer to compile from source, you will need to follow the instructions located [here.](https://github.com/tari-project/tari#building-from-source)

2. Use the instructions here to [install the binaries](https://github.com/tari-project/tari?tab=readme-ov-file#installing-using-binaries).

> NOTE: Depending on your environment, the location of the installed files will likely change. For Mac and Linux, you will likely find it in your Home directory in a `.tari` folder. It may be hidden, in which case you will need to change your settings to be able to view hidden files. On Windows, it will install on in the location you specified during the installation process. To have Minotari create the folder in a specific location, you can use the `--b` path command. Note that you will require this command going forward if you are not using the default folder.

The following binaries will be available.

* minotari_console_wallet
* minotari_merge_mining_proxy
* minotari_miner
* minotari_node
* randomx-benchmark
* randomx-codegen
* randomx-tests

The two required for the exchange are the **minotari_node** and **minotari_console_wallet**

3. Start the node (consecutive runs): 
```
minotari_node
```

If a node has not yet been created, it will inform you that a node config file does not exist. You will also be asked if you wish to mine. Select `n` in this case.

4. Next, you'll be asked if you wish to create a node identity. Select `y`. This is essential for generating the private/public key pair and getting the node recognised by the network.

Once done, the Minotari base node will boot up. You'll see a splash page with a list of the various Command Mode (accessible via Ctrl+C) commands available to you. Some useful ones are:

* `watch status`: returns you to the auto-refresh status from the Command Mode
* `version`: which version of the Minotari Node you are running
* `whoami`: provides address information related to the node

5. Type `whoami` and press enter. You'll see your Public Key, Node ID and Public Address, along with a QR Code. You should copy this data to a file or secure location for future reference.

```
18:46 v1.0.0-pre.16 esmeralda State: Listening Tip: 3872 (Tue, 23 Jul 2024 14:27:53 +0000) Mempool: 0tx (0g, +/- 0blks) Connections: 0|0 Banned: 0 Messages (last 60s): 0 Rpc: 0/100 ️🔌
>> whoami
Public Key: 90f67a04edcb36261e6304ca213629d183c44e26bd47e38c253473f44d901733
Node ID: e8ed9a4fd38577b6b01e3b8e9d
Public Addresses: /onion3/f5qbkkfkoxowzvshe5mppzpgiiy76cwumpsacungeldoal6hehcgzfqd:18141
Features: PeerFeatures(MESSAGE_PROPAGATION | DHT_STORE_FORWARD)
```

6. Restart the node (Ctrl+C twice to quit, then typing `minotari_node` again).

### Section 2: Creating a wallet
In this section we'll create a wallet address for receiving funds. This wallet will serve as the main repository of your Tari coins.

> NB: This is a crucial step in the process. Creating the wallet in secure environment and following the instructions is important to secure this wallet and prevent malicious actors being able to transfer Tari. Read the instructions carefully. If there is any doubt regarding ANY part of the process, please contact the Tari Community for clarification and assistance.

The Minotari wallet creation process is reliant on a seed word phrase to generate the associated master key. This seed phrase also allows for the recovery of the wallet. The seed phrase is a 24-word phrases generated from a pre-defined list of words, which will be displayed during the process.

> **WARNING: It is highly recommended this process is performed on a trusted machine that is disconnected from any other device or the Internet. The utmost caution should be taken in creating the wallet and noting and securing the seed phrase.**

1. First, let's create a folder to hold all the wallet data.
```
mkdir tari_wallet_data
cd tari_wallet_data
```

> Note: At multiple points in the sections covering the creation of the wallets, you will be directed to copy or note seed phrases, keys and other information. Do not store any of these within the created folder above, as you will need to delete this folder permanently in later steps.

2. Now let's run the wallet. Make sure that you specify the `--base-path` field to keep all the data in the above folder so that you can delete it afterwards.
```
minotari_console_wallet --base-path ~/tari_wallet_data
```

3. You'll be presented with a menu. As this guide assumes you are setting up Minotari for the first time, select `1`

```
Console Wallet

1. Create a new wallet.
2. Recover wallet from seed words or hardware device.
3. Create a read-only wallet using a view key.
>>
```

4. You will be asked if you want to mine. Choose `n`

```
Node config does not exist.
Would you like to mine (Y/n)?
NOTE: this will enable additional gRPC methods that could be used to monitor and submit blocks from this node.
```

5. You will be asked if you wish to use a connected hardware wallet. Press `n` here.

```
Would you like to use a connected hardware wallet? (Supported types: Ledger) (Y/n)
```

6. Next you will be asked for a password. You will need to save this password for future use. Enter this password now, and then again to confirm it. Be meticulous when doing so. We recommend following best practices to generating a strong password.

> NOTE: You will not see the password as you type it.

7. **This next step is vital. Be sure that no information leaks and that the seed phrase is only visible to yourself and/or trusted parties.** Following entry of the password, you will be presented with your seed words. Carefully note the seed words, write them down and secure them. Make sure you have appropriate, equally secure backups. You will only be able to proceed to the next step once you have typed `confirm` and pressed `Enter`.

```
=========================
       IMPORTANT!        
=========================
These are your wallet seed words.
They can be used to recover your wallet and funds.
WRITE THEM DOWN OR COPY THEM NOW. THIS IS YOUR ONLY CHANCE TO DO SO.

=========================
<...............seed words will be presented here.............>
=========================

I confirm that I will never see these seed words again.
Type the word "confirm" to continue.
>>
```

8. At this point, the Minotari wallet will launch in the console interface.

> Note: The following sections deal with configuration of the wallet. While not necessary, an extra safety precaution would be to confirm that the seed words you copied can actually recover the wallet.

### Section 3: Obtain the addresses of the main wallet

Now that we have the wallet created, we will require the addresses - specifically, the `Tari Address one-sided` - to create the second wallet, which will be used to monitor transactions.

If you followed the instructions from the previous section, you should already be in the Minotari console wallet interface. If not, run `minotari_console_wallet --base-path ~/tari_wallet_data` and enter in your password to launch the interface.

1. While in the wallet interface, press the right arrow twice to get to the `Receive` tab. This tab will list all of the addresses associated with the wallet.

<img src="../assets/lessons/img/tariexchangeguide/tariexchangeguide_wallet_addresses.png" width=400>

2. Copy all of the information provided, with special note of the `Tari Address one-sided` field. This is the address that users will send funds to for the exchange.

3. Press `f10` or `Ctrl+Q` to exit the wallet

4. Next, we'll export the view key for the wallet (We'll use this in **Section 4**). Run the following command and enter in your wallet password when prompted.

```
minotari_console_wallet --base-path ~/tari_wallet_data export-view-key-and-spend-key
```

You'll be presented with information that looks similar to the below:

```
1. ExportViewKeyAndSpendKey(ExportViewKeyAndSpendKeyArgs { output_file: None })

View key: cb6c13f07af23380c7756bbfcd622bc3277ec2cc42abd5ed3d8ddd19fa49060c
Spend key: f29039796b3430c6927f26bf216b6241dd7fad7f30a6640e8ac95f3d0af51a52
Minotari Console Wallet running... (Command mode completed)

Press Enter to continue to the wallet, or type q (or quit) followed by Enter.
```

5. Make note of the `view key` and `spend key`; copy them to an easily referenced place. We will require them in later steps.

6. Type `q` and then press `Enter` to exit the console wallet.

7. Make sure you have saved the above data. Permanently delete the folder `tari_wallet_data` and consider destroying or securely wiping the machine.

> Note: Now is a good time to check your noted keys, seed words and addresses before remove the configuration data in the folder and/or destroying/wiping the device.

### Section 4: Setting up a read-only wallet to receive deposits
In this section, we will create a second, read-only wallet that will watch for funds received at the address saved in the previous section. If you are integrating an exchange, this is how you can watch for received funds. This wallet will need to be able to access the Internet in some capacity.

> NOTE: This second wallet will not have the ability to spend any funds. While this limits the security risk, it is good practice to maintain security best practices when configuring any system that has access to the chain and has some association with the the main wallet.

1. On a server machine that is connected to the internet. Run this command `minotari_console_wallet` to create a wallet.

> Note: By default all data is stored in `~/.tari`. You can find all logs, config and data in there. If you would like to use a specific folder, you can use the `--base-path` argument to point to an existing folder or one you've created prior for this purpose.

2. You will be asked if you want to mine. Choose `N`

```
Node config does not exist.
Would you like to mine (Y/n)?
NOTE: this will enable additional gRPC methods that could be used to monitor and submit blocks from this node.
```

3. Next, you will be asked if you want to create a new wallet, restore it, or create a read-only wallet using a view key. We want to create a _read-only wallet_, so we will select `3` here.

```
Console Wallet

1. Create a new wallet.
2. Recover wallet from seed words or hardware device.
3. Create a read-only wallet using a view key.
>>
```

4. Next we will be asked for a password. You will need to save this password for future use. Enter this password now and confirm it. 

> Note: It is suggested you use a different password here from the one used to create the first wallet.

5. You will need to enter the view and spend keys noted in **Section 4**

```
Enter view key:  (hex)
<...view key here...>

Enter the public spend key:  (hex or base58)
<...public spend key here...>  
```

6. You should now see the familiar console wallet. We'll need to configure it further in its accompanying configuration file, so close it for now by pressing `f10` or `Ctrl+Q` and move onto the next section.

### Section 5: Configuring the read-only wallet
1. Browse to the config file under `~/.tari/mainnet/config/config.toml` (or the folder where you specified the wallet configuration should be stored) and open it in your favourite text editor.

2. Find the section `Wallet Configuration Options (WalletConfig)`. Below is a typical example of the beginning of the wallet configuration section within the `config.toml` file.

```toml
########################################################################################################################
#                                                                                                                      #
#                      Wallet Configuration Options (WalletConfig)                                                     #
#                                                                                                                      #
########################################################################################################################

[wallet]
# The buffer size constants for the publish/subscribe connector channel, connecting comms messages to the domain layer:
# (min value = 300, default value = 50000).
#buffer_size = 50000is
```

3. Next, find the line `#grpc_enabled = false` and change it to `grpc_enabled = true`. You will also need to uncomment the `grpc_address`.

> Note: If you wish to secure the gRPC more, you can edit the other settings, such as the `grpc_authentication`. It is important that the wallet's gRPC port is not accessible from the public internet

```toml
# Set to true to enable grpc. (default = false)
grpc_enabled = true
# The socket to expose for the gRPC base node server (default = "/ip4/127.0.0.1/tcp/18143")
grpc_address = "/ip4/127.0.0.1/tcp/18143"
# gRPC authentication method (default = "none")
#grpc_authentication = { username = "admin", password = "xxxx" }
```

4. Set the wallet's base node. Set this value to the `minotari_node` you created or chose at the beginning of this guide in **Section 1**.

> Note: The format is `<...public key...>::<...public address...>`, with <...> being replaced with the addresses noted previously. Below is a sample of what these configuration settings look like, using the example data from **Section 1**. You should not use the data below, but insert your own details.

```toml
# A custom base node peer that will be used to obtain metadata from, example
# "0eefb45a4de9484eca74846a4f47d2c8d38e76be1fec63b0112bd00d297c0928::/ip4/13.40.98.39/tcp/18189"
# (default = )
custom_base_node = "22d33b525d35d256674c5184c262b70d15275effcf5f6fe6dc0d359a18541d04::/onion3/6x54mmubphz5r3opswpuhseswivvlaxbohuqvwsn4o36zmtudq73dgid:18141"
```

5. Save the file and start the wallet again.

```
minotari_console_wallet
```

You are now ready to receive deposits. In the next section we'll explain how to listen for incoming transactions.

### Section 6: Listening for incoming transactions
How you listen for incoming transactions (and what you do with them) will depend on your process. For our example, we'll use the gRPC server that is hosted in the read-only wallet we just created to listen for incoming deposits. 

Reach out to us if you would like an example in your favourite language. You can find more information about the methods available in [wallet.proto here](https://github.com/tari-project/tari/blob/development/applications/minotari_app_grpc/proto/wallet.proto).

```javascript
const grpc = require('@grpc/grpc-js');
const protoLoader = require('@grpc/proto-loader');

// Load the protobuf
const PROTO_PATH = './proto/wallet.proto';
const packageDefinition = protoLoader.loadSync(PROTO_PATH, {
    keepCase: true,
    longs: String,
    enums: String,
    defaults: true,
    oneofs: true
});
const streamingProto = grpc.loadPackageDefinition(packageDefinition).tari.rpc;

// Create a client
console.log(streamingProto);
const client = new streamingProto.Wallet('localhost:18143', grpc.credentials.createInsecure());

const request = {};

// Call the gRPC method
const call = client.GetCompletedTransactions(request);

// Handle the stream of responses
call.on('data', (response) => {
    console.log('Received data:', response);
    // ..... Do business logic with transaction. E.g. compare the reference in payment_id to a reference provided to the exchange client and allocate
    // to their account
    // ....
});

call.on('end', () => {
    console.log('Stream ended.');
});

call.on('error', (err) => {
    console.error('Stream error:', err);
});

call.on('status', (status) => {
    console.log('Stream status:', status);
});
```

This is a basic implementation; some additional items you may want to consider for a production environment are: 

* Using `grpc.credentials.createSsl()` to secure the connection between the wallet and any application calling it. We'll not discuss the process of creating a server key or certificate here; you can read more about the process [here](https://www.ibm.com/docs/en/api-connect/10.0.x?topic=profile-generating-self-signed-certificate-using-openssl). Below is an example:

```javascript
// Load the protobuf definition
const PROTO_PATH = './proto/wallet.proto';
const packageDefinition = protoLoader.loadSync(PROTO_PATH, {
    keepCase: true,
    longs: String,
    enums: String,
    defaults: true,
    oneofs: true
});
const streamingProto = grpc.loadPackageDefinition(packageDefinition).tari.rpc;

// Read the server's certificate (server.crt)
const serverCert = fs.readFileSync('path/to/server.crt'); // Specify the path to your server's certificate

// Create secure credentials for the client using the server's certificate
const credentials = grpc.credentials.createSsl(serverCert);

// Create the gRPC client with the secure credentials
const client = new streamingProto.Wallet('localhost:18143', credentials);

// Prepare the request object (you can modify this based on the method's requirements)
const request = {};

// Call the gRPC method with secure connection
const call = client.GetCompletedTransactions(request);

// Handle the stream of responses
call.on('data', (response) => {
    console.log('Received data:', response);
    // Process transaction data here. Example:
    // Compare the reference in payment_id to a reference provided to the exchange client and allocate to their account
});

call.on('end', () => {
    console.log('Stream ended.');
});

call.on('error', (err) => {
    console.error('Stream error:', err);
});

call.on('status', (status) => {
    console.log('Stream status:', status);
});
```

## Descriptions of Common Activities
### Section 7: An example for receiving funds
Each exchange will have their own processes, but here is an example of receiving funds from a KYC'ed client. 

1. The client begins the deposit process. For example, clicking on a "Deposit" button.

2. The exchange generates a long unique ID for the deposit. This may be a single reference that is reused for the client, or every deposit may have their own reference.

3. The exchange provides their `Tari Address one-sided` address and the reference to the client. The exchange must also save this reference in their internal database.

> Note: Exchanges should use the one-sided or non-interactive addresses so they can receive deposits even if their infrastructure is offline. Interactive addresses are intended for peer-to-peer transactions.

4. The client uses Tari Aurora or another Tari-enabled wallet and sends a non-interactive transaction to the provided address. They must include the provided reference with this transaction.

> Note: Using the Minotari console wallet, for example, the recommendation would be for the user to place your payment reference in the `Payment ID` field.

5. A process similar to the example in **Section 6**, the exchange periodically runs the script to see if there are any new transactions.

6. For new transactions, compare against the list of expected references in their internal database and if there is a match, call the internal system to allocate funds to the client's account.

### Section 8: Performing withdrawals

In this section we'll perform a withdrawal from the same address we used in **Section 3**. It is also possible to have a number of different wallets and send funds between them. The process is mostly the same, but is out of scope for this document.

> NOTE: The wallet used to spend funds should not be online for more time than is necessary. It is recommended that the machine running this wallet is secured.

Before we spend funds, we must have a wallet set up with the seed words created in Step 7 of **Section 2**.

Once the wallet is set up, continue with the steps below.

1. Run the wallet to update the balance

```
minotari_console_wallet --password <password> -p "wallet.custom_base_node=<...node_pub_key...>::<...node_pub_address...>" --auto-exit sync
```

> Note: The custom base node can also be set as an environment variable `TARI_WALLET__CUSTOM_BASE_NODE`

2. Validate there are sufficient funds in the wallet
```
minotari_console_wallet --password <password> -p "wallet.custom_base_node=<...node_pub_key...>::<...node_pub_address...>" --auto-exit get-balance
```

```
Minotari Console Wallet running... (Command mode started)
==============
Command Runner
==============

1. GetBalance

Available balance: 10000.000000 T
Time locked: 0 µT
Pending incoming balance: 27960.980255 T
Pending outgoing balance: 0 µT

Minotari Console Wallet running... (Command mode completed)
```

3. Next, send funds to the desired address.

```
minotari_console_wallet --password <password> -p "wallet.custom_base_node=<node_pub_key>::<node_pub_address>" --auto-exit send-minotari <amount> <destination>
```

Replace `<amount>` and `<destination>` with the amount to send and Tari address to send funds to. Note: The amount is specified in units of 0.000001 XTM. To specify the amount in Tari, you can append the letter `T`. For example, `send-minotari 10000` would send an amount of `0.01 XTM`. `send-minotari 10000T` would send an amount of `10000 XTM`.

Exchanges should not allow clients to provide interactive Tari Addresses. This can be easily validated by checking the second byte of the address. Specifically, the byte that represents an interactive wallet would be 01 in hexadecimal, or 00000001 in binary.

To break it down:
* The value is 01 (hexadecimal)
* In binary, this is 00000001
* The least significant bit (rightmost bit) is 1, indicating support for interactive transactions