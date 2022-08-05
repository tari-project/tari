// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const WalletProcess = require("integration_tests/helpers/walletProcess");
const WalletClient = require("integration_tests/helpers/walletClient");
const { getFreePort } = require("integration_tests/helpers/util");
const { sleep } = require("integration_tests/helpers/util");
const { PaymentType } = require("integration_tests/helpers/types");
const debug = require("debug")("washing-machine");
const { DateTime, Interval } = require("luxon");
const { sendWebhookNotification, getWebhookUrl, yargs } = require("./helpers");

// FPG to use for transactions
const FEE_PER_GRAM = 5;
const WEBHOOK_CHANNEL = "protocol-bot-stuff";

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
      default: Infinity,
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
    .option("routing-mechanism", {
      alias: "r",
      type: "string",
      default: "DirectAndStoreAndForward",
      description:
        "Possible values: DirectOnly, StoreAndForwardOnly, DirectAndStoreAndForward.",
    })
    .alias("help", "h");

  argObj.help();

  const webhookNotificationsEnabled = !!getWebhookUrl();
  if (!webhookNotificationsEnabled) {
    console.warn(
      "Matter most notifications are disabled because WEBHOOK_URL environment variable is not set"
    );
  }

  let washingMachine = WashingMachine.new({
    webhookNotificationsEnabled,
    ...argObj.argv,
  });
  try {
    await washingMachine.run();
  } catch (err) {
    console.error(err);
    logNotify(`🚨 Washing machine failed: ${err.toString()}`);
  }
}

function WashingMachine(options) {
  debug(`Washing machine initialized - ${JSON.stringify(options, null, 2)}`);

  this.wallet1 = new WalletClient();
  this.wallet2 = null;

  this.showWalletDetails = async function () {
    for (let [name, wallet] of [
      ["Wallet 1", this.wallet1],
      ["Wallet 2", this.wallet2],
    ]) {
      const walletIdentity = await wallet.identify();
      const { status, num_node_connections } = await wallet.getNetworkStatus();
      console.log(
        `${name}: ${walletIdentity.public_key} status = ${status}, num_node_connections = ${num_node_connections}`
      );
    }
  };

  this.run = async () => {
    const {
      baseNode: baseNodeSeed,
      numTxns: numTransactions,
      oneSided,
      numRounds,
      sleepAfterRound,
      wallet1Grpc,
      wallet2Grpc,
      amountRange,
      routingMechanism,
    } = options;

    logNotify(
      `🚀 Launching washing machine (numTransactions = ${numTransactions}, numRounds = ${numRounds}, sleep = ${sleepAfterRound}s)`
    );

    debug(`Connecting to wallet1 at ${wallet1Grpc}...`);
    await this.wallet1.connect(wallet1Grpc);
    debug(`Connected.`);

    let wallet2Process = null;
    // Start wallet2
    if (wallet2Grpc) {
      this.wallet2 = new WalletClient();
      debug(`Connecting to wallet2 at ${wallet2Grpc}...`);
      await this.wallet2.connect(wallet2Grpc);
      debug(`Connected.`);
    } else {
      debug("Compiling wallet2...");
      const port = await getFreePort(20000, 25000);
      wallet2Process = createGrpcWallet(
        baseNodeSeed,
        {
          routingMechanism,
          grpc_console_wallet_address: `127.0.0.1:${port}`,
        },
        true
      );
      wallet2Process.baseDir = "./wallet";
      debug("Starting wallet2...");
      await wallet2Process.startNew();
      this.wallet2 = await wallet2Process.connectClient();
    }

    await this.showWalletDetails();

    let [minAmount, maxAmount] = amountRange
      .split("-", 2)
      .map((n) => parseInt(n) * 1000);

    const minRequiredBalance =
      ((maxAmount - minAmount) / 2) * numTransactions +
      calcPossibleFee(FEE_PER_GRAM, numTransactions);

    let currentBalance = await waitForBalance(this.wallet1, minRequiredBalance);

    debug(
      `Required balance (${minRequiredBalance}uT) reached: ${currentBalance.available_balance}uT. Sending transactions between ${minAmount}uT and ${maxAmount}uT`
    );

    let roundCount = 0;
    let counters = { numSuccess: 0, numFailed: 0 };
    const startTime = DateTime.now();
    let lastNotifiedAt = DateTime.now();
    for (;;) {
      debug(`Wallet 1 -> Wallet 2`);
      {
        let {
          totalSent: wallet1AmountSent,
          numSuccess,
          numFailed,
        } = await sendFunds(this.wallet1, this.wallet2, {
          minAmount: minAmount,
          maxAmount: maxAmount,
          oneSided,
          numTransactions,
          feePerGram: FEE_PER_GRAM,
        });

        counters.numSuccess += numSuccess;
        counters.numFailed += numFailed;
        debug(
          `Waiting for wallet2 to have a balance of ${wallet1AmountSent}uT`
        );

        await waitForBalance(this.wallet2, wallet1AmountSent);
      }

      debug(`Wallet 2 -> Wallet 1`);
      const {
        totalSent: wallet2AmountSent,
        numSuccess,
        numFailed,
      } = await sendFunds(this.wallet2, this.wallet1, {
        minAmount: minAmount,
        maxAmount: maxAmount,
        oneSided,
        numTransactions,
        feePerGram: FEE_PER_GRAM,
      });

      counters.numSuccess += numSuccess;
      counters.numFailed += numFailed;

      roundCount++;
      if (isFinite(numRounds) && roundCount >= numRounds) {
        break;
      }

      // Status update every couple of hours
      if (DateTime.now().diff(lastNotifiedAt).hours >= 4) {
        lastNotifiedAt = DateTime.now();
        const uptime = Interval.fromDateTimes(startTime, DateTime.now());
        const rate =
          (counters.numSuccess / (uptime.toDuration("seconds") * 1.0 || 1.0)) *
          60.0;
        const upstimeStr = uptime
          .toDuration(["days", "hours", "minutes", "seconds"])
          .toFormat("d'd' h'h' m'm' s's'");
        logNotify(
          `Washing machine status. ${counters.numSuccess} sent,` +
            `${counters.numFailed} failed in ${roundCount} rounds. ` +
            `Uptime: ${upstimeStr}, Avg. rate: ${rate.toFixed(2)}txs/m`
        );
      }

      if (sleepAfterRound) {
        debug(`Taking a break for ${sleepAfterRound}s`);
        await sleep(sleepAfterRound * 1000);
      }

      await waitForBalance(this.wallet1, wallet2AmountSent);
    }

    const uptime = Interval.fromDateTimes(startTime, DateTime.now());
    logNotify(
      `Washing machine completed. ${counters.numSuccess} sent, ${counters.numFailed} failed in ${numRounds} rounds. Uptime is ${uptime}`
    );
    if (wallet2Process) {
      await wallet2Process.stop();
    }
  };
}

WashingMachine.new = (...args) => new WashingMachine(...args);

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
    let avgAmountPerTransaction = Math.floor(totalToSend / transactions.length);
    debug(`COINSPLIT: amount = ${avgAmountPerTransaction}uT`);
    if (transactions.length > 1) {
      let leftToSplit = transactions.length;
      while (leftToSplit > 499) {
        // Wait for balance + 1t leeway for fees
        await waitForBalance(
          senderWallet,
          avgAmountPerTransaction * 499 + 1000000
        );
        let split_result = await senderWallet.coin_split({
          amount_per_split: avgAmountPerTransaction,
          split_count: 499,
          fee_per_gram: options.feePerGram,
        });
        debug(`Split: ${JSON.stringify(split_result)}`);
        leftToSplit -= 499;
      }
      if (leftToSplit > 0) {
        // Wait for balance + 1t leeway for fees
        await waitForBalance(
          senderWallet,
          avgAmountPerTransaction * leftToSplit + 1000000
        );
        let split_result = await senderWallet.coin_split({
          amount_per_split: avgAmountPerTransaction,
          split_count: leftToSplit,
          fee_per_gram: options.feePerGram,
        });
        debug(`Last Split: ${JSON.stringify(split_result)}`);
      }
    }
    await waitForBalance(senderWallet, totalToSend);
  }

  let { results } = await senderWallet.transfer({
    recipients: transactions,
  });

  let numSuccess = 0;
  let numFailed = 0;
  for (const result of results) {
    if (result.is_success) {
      numSuccess += 1;
      debug(`✅  transaction #${result.transaction_id}`);
    } else {
      numFailed += 1;
      debug(
        `❌ ${result.failure_message} transaction #${result.transaction_id}`
      );
    }
  }

  // TODO: total sent should take failures into account
  return { totalSent: totalToSend, numSuccess, numFailed };
}

function* transactionGenerator(options) {
  // Loosely account for fees
  const avgSpendPerTransaction =
    options.minAmount +
    (options.maxAmount - options.minAmount) / 2 +
    calcPossibleFee(options.feePerGram, 1);
  debug(
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

function createGrpcWallet(baseNode, opts = {}, excludeTestEnvars = true) {
  let process = new WalletProcess("sender", excludeTestEnvars, {
    transport: "tor",
    network: "esmeralda",
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
  let r = 1;
  let newBalance = await client.getBalance();
  debug(
    `Waiting for available wallet balance (${newBalance.available_balance}uT, pending=${newBalance.pending_incoming_balance}uT) to reach at least ${balance}uT...`
  );
  for (;;) {
    newBalance = await client.getBalance();
    if (newBalance.available_balance >= balance) {
      return newBalance;
    }
    await sleep(1000);
    if (i >= 60) {
      debug(
        `Still waiting... [t=${r * i}s, balance=${
          newBalance.available_balance
        }, pending=${newBalance.pending_incoming_balance}]`
      );
      i = 0;
      r++;
    } else {
      i++;
    }
  }
}

function collect(iter) {
  let arr = [];
  for (let i of iter) {
    arr.push(i);
  }
  return arr;
}

function calcPossibleFee(feePerGram, numTransactions) {
  const TRANSACTION_WEIGHT = (1 + 3 + 13) * 2;
  return feePerGram * TRANSACTION_WEIGHT * numTransactions;
}

function logNotify(message) {
  console.log(message);
  sendWebhookNotification(WEBHOOK_CHANNEL, message);
}

Promise.all([main()]);
