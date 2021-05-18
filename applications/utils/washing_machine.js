

const WalletProcess = require("../../integration_tests/helpers/walletProcess");
const WalletClient = require("../../integration_tests/helpers/walletClient");
const {sleep} = require("../../integration_tests/helpers/util");


/// To start, create a normal console wallet, and send it funds. Then run it with GRPC set to 18143. For quickest results,
/// set the confirmation time to 0 (must be mined)
/// This test will send txtr between two wallets and then back again. It waits for a block to be mined, so could take a while
async function main() {
 console.log("Hello, starting the washing machine")

    // ncal
    let baseNode = "e2cef0473117da34108dd85d4425536b8a1f317478686a6d7a0bbb5c800a747d::/onion3/3eiacmnozk7rcvrx7brhlssnpueqsjdsfbfmq63d2bk7h3vtah35tcyd:18141";

    // load sender wallet
    let senderClient= new WalletClient("127.0.0.1:18143")

    let receiverWallet = createGrpcWallet(baseNode);
 receiverWallet.baseDir = "./wallet";
    await receiverWallet.startNew();

    let receiverClient = receiverWallet.getClient();

    await doSend(senderClient, receiverClient, 1000*1000);
    // Send back
    await doSend(receiverClient, senderClient, 900*1000);
    await receiverWallet.stop();
}


async function doSend(senderClient, receiverClient, amount) {
    const MAX_TRANSACTIONS = 1000;
    let receiverInfo = await receiverClient.identify();
    console.log(receiverInfo);
    for (let numberTransactions = 1000; numberTransactions <= MAX_TRANSACTIONS; numberTransactions++) {
        if (numberTransactions > 1) {
            let leftToSplit = numberTransactions;
            while (leftToSplit >499) {
                let split_result = await senderClient.coin_split({
                    amount_per_split: amount / numberTransactions,
                    split_count: 499,
                    fee_per_gram: 10,
                });
                console.log("Split:", split_result);
                leftToSplit-= 499;
            }
            let split_result = await senderClient.coin_split({
                amount_per_split: amount / numberTransactions,
                split_count: leftToSplit,
                fee_per_gram: 10,
            });
            console.log("Split:", split_result);
        }

        // wait for balance to become available
        for (let i = 0; i< 20*60; i++) {

            let newBalance = await senderClient.getBalance();
            console.log(`Waiting for new balance (spender) ${newBalance.available_balance} to be more than required amount: ${amount}. Pending ${newBalance.pending_balance}`);
            if (parseInt(newBalance.available_balance) >= amount) {
                break;
            }
            await sleep(1000);
        }
        let senderBalance = await senderClient.getBalance();
        console.log("Sender: ", senderBalance);
        let balance = await receiverClient.getBalance();
        console.log("Balance:", balance);

        for (let i =0; i < numberTransactions; i++) {
            let results = await senderClient.transfer({
                recipients: [{
                    address: receiverInfo.public_key,
                    amount: amount / numberTransactions,
                    fee_per_gram: 5,
                    message: `Washing machine funds ${i} of ${numberTransactions}`
                }]
            });
            console.log("results", results);
        }

        // wait for balance
        for (let i = 0; i< 20*60; i++) {

            let newBalance = await receiverClient.getBalance();
            console.log("new Balance:", newBalance);
            if (parseInt(newBalance.available_balance) > parseInt(balance.available_balance)) {
                break;
            }
            await sleep(1000);
        }
    }
}

function createGrpcWallet(baseNode) {
    let process = new WalletProcess("sender", {transport:"tor", network: "stibbons", num_confirmations: 0});
    process.setPeerSeeds([baseNode]);
    return process;
}

Promise.all([main()]);
