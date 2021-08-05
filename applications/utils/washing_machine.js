const WalletProcess = require("../../integration_tests/helpers/walletProcess");
const WalletClient = require("../../integration_tests/helpers/walletClient");
const {getFreePort} = require("../../integration_tests/helpers/util");
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
    .option("routing-mechanism", {alias: "r", type: "string", default: "StoreAndForwardOnly"})
    .option("web-hook", {
       type: "string",
       default: null,
       description: "Optional web hook to send messages to instead of outputting to console (requires cURL).",
    })
    .alias("help", "h");

  //todo: Make channel configurable for web-hook

  argObj.help();

  const { argv: args } = argObj;
  const hook = args.webHook;

  debug(hook, JSON.stringify(args, null, 2));

  log_notify("Hello, starting the washing machine", hook);

  log_notify("Compiling and starting applications...", hook);
  let wallet1 = new WalletClient(args.wallet1Grpc);
  // Start wallet2
  let wallet2;
  let wallet2Process = null;
  if (!args.wallet2Grpc) {
    const port = await getFreePort(20000, 25000);
    wallet2Process = createGrpcWallet(args.baseNode, {routingMechanism: args.routingMechanism, grpc_console_wallet_address:`127.0.0.1:${port}`}, true);
    wallet2Process.baseDir = "./wallet";
    await wallet2Process.startNew();
    wallet2 = wallet2Process.getClient();
  } else {
    wallet2 = new WalletClient(args.wallet2Grpc);
  }

  await showWalletDetails("Wallet 1", wallet1, hook);
  await showWalletDetails("Wallet 2 ", wallet2, hook);

  let [minAmount, maxAmount] = args.amountRange
    .split("-", 2)
    .map((n) => parseInt(n) * 1000);

  const { numTxns: numTransactions, numRounds, sleepAfterRound } = args;

  const minRequiredBalance =
    ((maxAmount - minAmount) / 2) * numTransactions +
    calcPossibleFee(FEE_PER_GRAM, numTransactions);

  let currentBalance = await waitForBalance(wallet1, minRequiredBalance, hook);

  log_notify(
    `Required balance (${minRequiredBalance}uT) reached: ${currentBalance.available_balance}uT. Sending transactions between ${minAmount}uT and ${maxAmount}uT`, hook
  );

  let roundCount = 0;
  while (true) {
    log_notify(`Wallet 1 -> Wallet 2`, hook);
    let wallet1AmountSent = await sendFunds(wallet1, wallet2, {
      minAmount: minAmount,
      maxAmount: maxAmount,
      oneSided: args.oneSided,
      numTransactions,
      feePerGram: FEE_PER_GRAM,
    }, hook);

    log_notify(
      `Waiting for wallet2 to have a balance of ${wallet1AmountSent}uT`, hook
    );
    await waitForBalance(wallet2, wallet1AmountSent, hook);

    log_notify(`Wallet 2 -> Wallet 1`, hook);
    await sendFunds(wallet2, wallet1, {
      minAmount: minAmount,
      maxAmount: maxAmount,
      oneSided: args.oneSided,
      numTransactions,
      feePerGram: FEE_PER_GRAM,
    }, hook);

    roundCount++;
    if (numRounds && roundCount >= numRounds) {
      break;
    }
    if (sleepAfterRound) {
      log_notify(`Taking a break for ${sleepAfterRound}s`, hook);
      await sleep(sleepAfterRound * 1000);
    }
  }

  if (wallet2Process) {
    await wallet2Process.stop();
  }
}

async function sendFunds(senderWallet, receiverWallet, options, hook) {
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
  }, hook);

  let transactions = collect(transactionIter);
  let totalToSend = transactions.reduce(
    (total, { amount }) => total + amount,
    0
  );
  // For interactive transactions, a coin split is needed first
  if (!options.oneSided) {
    let avgAmountPerTransaction = totalToSend / transactions.length;
    log_notify(`COINSPLIT: amount = ${avgAmountPerTransaction}uT`, hook);
    if (transactions.length > 1) {
      let leftToSplit = transactions.length;
      while (leftToSplit > 499) {
        let split_result = await senderWallet.coin_split({
          amount_per_split: avgAmountPerTransaction,
          split_count: 499,
          fee_per_gram: options.feePerGram,
        });
        log_notify(`Split: ${JSON.stringify(split_result)}`, hook);
        leftToSplit -= 499;
      }
      if (leftToSplit > 0) {
        let split_result = await senderWallet.coin_split({
          amount_per_split: avgAmountPerTransaction,
          split_count: leftToSplit,
          fee_per_gram: options.feePerGram,
        });
        log_notify(`Last Split: ${JSON.stringify(split_result)}`, hook);
      }
    }
    await waitForBalance(senderWallet, totalToSend, hook);
  }

  let { results } = await senderWallet.transfer({
    recipients: transactions,
  });
  // debug(results);
  for (let result of results) {
    log_notify(
      `${
        result.is_success ? "✅  " : `❌ ${result.failure_message}`
      } transaction #${result.transaction_id} `, hook
    );
  }

  return totalToSend;
}

function* transactionGenerator(options, hook) {
  // Loosely account for fees
  const avgSpendPerTransaction =
    options.minAmount +
    (options.maxAmount - options.minAmount) / 2 +
    calcPossibleFee(options.feePerGram, 1);
  log_notify(
    `Generating ${options.numTransactions} transactions averaging ${avgSpendPerTransaction}uT (incl fees)`
  , hook);

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

function createGrpcWallet(baseNode, opts = {}, excludeTestEnvars= true) {
  let process = new WalletProcess("sender", excludeTestEnvars, {
    transport: "tor",
    network: "weatherwax",
    num_confirmations: 0,
    ...opts,
  });
  process.setPeerSeeds([baseNode]);
  return process;
}

async function waitForBalance(client, balance, hook) {
  if (isNaN(balance)) {
    log_notify("Error: balance is not a number", hook);
    throw new Error("balance is not a number");
  }
  let i = 0;
  let r = 1;
  let newBalance = await client.getBalance();
  log_notify(
      `Waiting for available wallet balance (${newBalance.available_balance}uT, pending=${newBalance.pending_incoming_balance}uT) to reach at least ${balance}uT...`
      , hook);
  while (true) {
    newBalance = await client.getBalance();
    if (newBalance.available_balance >= balance) {
      return newBalance;
    }
    await sleep(1000);
    if (i >= 60)
    {
      log_notify(`Still waiting... [t=${r*i}s]`,hook);
      i = 0;
      r++;
    } else {
      i++;
    }
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

function debug(hook, ...args) {
  if (process.env.DEBUG) {
    log_notify(...args, hook);
  }
}

function log_notify(message, hook) {
  if (!hook) {
    console.log(message);
  } else {
    hook_notify(message, hook);
  }
}

function hook_notify(message, hook) {
  const exec = require('child_process').exec;
  const data = `{"text": "${message}", "channel": "protocol-bot-stuff"}`;
  const args = ` -i -X POST -H 'Content-Type: application/json' -d '${data}' ${hook}`;
  exec('curl ' + args, function (error, stdout, stderr) {
    console.log('stdout: ' + stdout);
    console.log('stderr: ' + stderr);
    if (error !== null) {
      console.log('exec error: ' + error);
    }
  });
}

async function showWalletDetails(name, wallet, hook) {
  const walletIdentity = await wallet.identify();
  const { status, num_node_connections } = await wallet.getNetworkStatus();
  log_notify(`${name}: ${walletIdentity.public_key} status = ${status}, num_node_connections = ${num_node_connections}`, hook);
}

Promise.all([main()]);
