const WalletProcess = require("../../integration_tests/helpers/walletProcess");
const WalletClient = require("../../integration_tests/helpers/walletClient");
const { sleep, yargs } = require("../../integration_tests/helpers/util");
const { PaymentType } = require("../../integration_tests/helpers/types");

// FPG to use for transactions
const FEE_PER_GRAM = 5;

/// To start, create a normal console wallet, and send it funds. Then run it with GRPC set to 18143. For quickest results,
/// set the confirmation time to 0 (must be mined)
/// This test will send txtr between two wallets and then back again. It waits for a block to be mined, so could take a while
async function main() {
  const argObj = yargs()
    .option("base-node", {
      alias: "b",
      description:
        "Base node for wallet2. This is ignored if wallet2-grpc is set",
      type: "string",
      default:
        // ncal
        "e2cef0473117da34108dd85d4425536b8a1f317478686a6d7a0bbb5c800a747d::/onion3/3eiacmnozk7rcvrx7brhlssnpueqsjdsfbfmq63d2bk7h3vtah35tcyd:18141",
    })
    .option("wallet1-grpc", {
      alias: "w1",
      description: "Wallet 1 GRPC address",
      type: "string",
      default: "127.0.0.1:18143",
    })
    .option("wallet2-grpc", {
      alias: "w2",
      description:
        "Wallet 2 GRPC address (If not supplied, a new wallet will be started)",
      type: "string",
      default: null,
    })
    .option("num-txns", {
      alias: "t",
      type: "number",
      default: 10,
      description:
        "The number of transactions that are sent each way within a washing round.",
    })
    .option("num-rounds", {
      alias: "n",
      type: "number",
      default: null,
      description: "The number of send back and forth washing rounds.",
    })
    .option("amount-range", {
      type: "string",
      description: "The start/end range for per txn amounts in millitari (mT)",
      default: "50-200",
    })
    .option("sleep-after-round", {
      alias: "s",
      type: "number",
      description: "Interval in seconds between rounds",
      default: null,
    })
    .option("one-sided", { type: "boolean", default: false })
    .alias("help", "h");

  argObj.help();

  const { argv: args } = argObj;

  debug(JSON.stringify(args, null, 2));
  console.log("Hello, starting the washing machine");

  let wallet1 = new WalletClient();
  await wallet1.connect(args.wallet1Grpc);

  // Start wallet2
  let wallet2;
  let wallet2Process = null;
  if (!args.wallet2Grpc) {
    wallet2Process = createGrpcWallet(args.baseNode);
    wallet2Process.baseDir = "./wallet";
    await wallet2Process.startNew();
    wallet2 = await wallet2Process.connectClient();
  } else {
    wallet2 = new WalletClient();
    await wallet2.connect(args.wallet2Grpc);
  }

  await showWalletDetails("Wallet 1", wallet1);
  await showWalletDetails("Wallet 2 ", wallet2);

  let [minAmount, maxAmount] = args.amountRange
    .split("-", 2)
    .map((n) => parseInt(n) * 1000);

  const { numTxns: numTransactions, numRounds, sleepAfterRound } = args;

  const minRequiredBalance =
    ((maxAmount - minAmount) / 2) * numTransactions +
    calcPossibleFee(FEE_PER_GRAM, numTransactions);
  let currentBalance = await waitForBalance(wallet1, minRequiredBalance);

  console.log(
    `Required balance (${minRequiredBalance}uT) reached: ${currentBalance.available_balance}uT. Sending transactions between ${minAmount}uT and ${maxAmount}uT`
  );

  let roundCount = 0;
  while (true) {
    console.log(`Wallet 1 -> Wallet 2`);
    let wallet1AmountSent = await sendFunds(wallet1, wallet2, {
      minAmount: minAmount,
      maxAmount: maxAmount,
      oneSided: args.oneSided,
      numTransactions,
      feePerGram: FEE_PER_GRAM,
    });

    console.log(
      `Waiting for wallet2 to have a balance of ${wallet1AmountSent}uT`
    );
    await waitForBalance(wallet2, wallet1AmountSent);

    console.log(`Wallet 2 -> Wallet 1`);
    await sendFunds(wallet2, wallet1, {
      minAmount: minAmount,
      maxAmount: maxAmount,
      oneSided: args.oneSided,
      numTransactions,
      feePerGram: FEE_PER_GRAM,
    });

    roundCount++;
    if (numRounds && roundCount >= numRounds) {
      break;
    }
    if (sleepAfterRound) {
      console.log(`Taking a break for ${sleepAfterRound}s`);
      await sleep(sleepAfterRound * 1000);
    }
  }

  if (wallet2Process) {
    await wallet2Process.stop();
  }
}

async function sendFunds(senderWallet, receiverWallet, options) {
  const { available_balance: senderBalance } = await senderWallet.getBalance();
  const receiverInfo = await receiverWallet.identify();
  const paymentType = options.oneSided
    ? PaymentType.ONE_SIDED
    : PaymentType.STANDARD_MIMBLEWIMBLE;

  const transactionIter = transactionGenerator({
    address: receiverInfo.public_key,
    feePerGram: options.feePerGram,
    minAmount: options.minAmount,
    maxAmount: options.maxAmount,
    numTransactions: options.numTransactions,
    balance: senderBalance,
    paymentType,
  });

  let transactions = collect(transactionIter);
  let totalToSend = transactions.reduce(
    (total, { amount }) => total + amount,
    0
  );
  // For interactive transactions, a coin split is needed first
  if (!options.oneSided) {
    let avgAmountPerTransaction = totalToSend / transactions.length;
    console.log(`COINSPLIT: amount = ${avgAmountPerTransaction}uT`);
    if (transactions.length > 1) {
      let leftToSplit = transactions.length;
      while (leftToSplit > 499) {
        let split_result = await senderWallet.coin_split({
          amount_per_split: avgAmountPerTransaction,
          split_count: 499,
          fee_per_gram: options.feePerGram,
        });
        console.log("Split:", split_result);
        leftToSplit -= 499;
      }
      if (leftToSplit > 0) {
        let split_result = await senderWallet.coin_split({
          amount_per_split: avgAmountPerTransaction,
          split_count: leftToSplit,
          fee_per_gram: options.feePerGram,
        });
        console.log("Last split:", split_result);
      }
    }
    await waitForBalance(senderWallet, totalToSend);
  }

  let { results } = await senderWallet.transfer({
    recipients: transactions,
  });
  // debug(results);
  for (let result of results) {
    console.log(
      `${
        result.is_success ? "✅  " : `❌ ${result.failure_message}`
      } transaction #${result.transaction_id} `
    );
  }

  return totalToSend;
}

function* transactionGenerator(options) {
  // Loosely account for fees
  const avgSpendPerTransaction =
    options.minAmount +
    (options.maxAmount - options.minAmount) / 2 +
    calcPossibleFee(options.feePerGram, 1);
  console.log(
    `Generating ${options.numTransactions} transactions averaging ${avgSpendPerTransaction}uT (incl fees)`
  );

  let amountToSend = options.minAmount;
  let i = 0;
  let total = 0;
  while (i < options.numTransactions && total < options.balance) {
    total += amountToSend + calcPossibleFee(options.feePerGram, 1);
    yield {
      address: options.address,
      amount: amountToSend,
      fee_per_gram: options.feePerGram,
      message: `Washing machine funds ${i + 1} of ${options.numTransactions}`,
      payment_type: options.paymentType,
    };

    amountToSend = Math.max(
      options.minAmount,
      (amountToSend + 1) % options.maxAmount
    );
    i++;
  }
}

function createGrpcWallet(baseNode, opts = {}) {
  let process = new WalletProcess("sender", false, {
    transport: "tor",
    network: "stibbons",
    num_confirmations: 0,
    ...opts,
  });
  process.setPeerSeeds([baseNode]);
  return process;
}

async function waitForBalance(client, balance) {
  if (isNaN(balance)) {
    throw new Error("balance is not a number");
  }
  let i = 0;
  while (true) {
    let newBalance = await client.getBalance();
    console.log(
      `[t=${i}s] Waiting for available wallet balance (${newBalance.available_balance}uT, pending=${newBalance.pending_incoming_balance}uT) to reach at least ${balance}uT...`
    );
    if (newBalance.available_balance >= balance) {
      return newBalance;
    }
    await sleep(1000);
    i++;
  }
}

function collect(iter) {
  let arr = [];
  for (i of iter) {
    arr.push(i);
  }
  return arr;
}

function calcPossibleFee(feePerGram, numTransactions) {
  const TRANSACTION_WEIGHT = (1 + 3 + 13) * 2;
  return feePerGram * TRANSACTION_WEIGHT * numTransactions;
}

function debug(...args) {
  // Poor man's debug
  if (process.env.DEBUG) {
    console.log(...args);
  }
}

async function showWalletDetails(name, wallet) {
  const walletIdentity = await wallet.identify();
  const { status, num_node_connections } = await wallet.getNetworkStatus();
  console.log(
    `${name}: ${walletIdentity.public_key} status = ${status}, num_node_connections = ${num_node_connections}`
  );
}

Promise.all([main()]);
