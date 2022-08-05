// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const fs = require("fs").promises;
const yargs = require("yargs");
const path = require("path");
const helpers = require("./helpers");
const WalletProcess = require("integration_tests/helpers/walletProcess");

const RECOVERY_COMPLETE_REGEXP = /Recovery complete! Scanned (\d+) blocks in/;
const RECOVERY_WORTH_REGEXP = /worth ([0-9.]+) (µ?T)/;
const FAILURE_REGEXP =
  /Attempt (\d+)\/(\d+): Failed to complete wallet recovery/;

async function main() {
  const argv = yargs
    .option("seed-words", {
      alias: "sw",
      description: "Seed words to use during recovery",
      type: "string",
      default:
        "parade jelly sample worth bind release forest snack job mobile divide ranch fee raccoon begin awful source thank check leaf vibrant stove material field",
    })
    .option("log", {
      alias: "l",
      description: "output logs to this file",
      type: "string",
      default: "logs/wallet.log",
    })
    .option("num-wallets", {
      alias: "n",
      description: "The number of times a wallet instance is recovered",
      type: "integer",
      default: 1,
    })
    .help()
    .alias("help", "h").argv;

  for (let i = 0; i < argv.numWallets; i++) {
    let { identity, timeDiffMinutes, numOutputs, rate, recoveredAmount } =
      await run(argv);

    console.log(
      "Wallet (Pubkey:",
      identity.public_key,
      ") scanned",
      numOutputs,
      "outputs, completed in",
      timeDiffMinutes,
      "minutes (",
      rate,
      "outputs/min).",
      recoveredAmount,
      "µT recovered for instance ",
      i,
      "."
    );
  }
}

async function run(options = {}) {
  const wallet = new WalletProcess(
    "compile",
    true,
    {
      transport: "tor",
      network: "esmeralda",
      grpc_console_wallet_address: "/ip4/127.0.0.1/tcp/18111",
      baseDir: options.baseDir || "./temp/base-nodes/",
    },
    "../../integration_tests/log4rs/wallet.yml",
    options.seedWords
  );

  await wallet.startNew();

  let startTime = new Date();

  await fs.mkdir(path.dirname(options.log), { recursive: true });
  let logfile = await fs.open(options.log, "w+");

  try {
    let recoveryResult = await helpers.monitorProcessOutput({
      process: wallet.ps,
      outputStream: logfile,
      onData: (data) => {
        let scannedMatch = data.match(RECOVERY_COMPLETE_REGEXP);
        let recoveredAmountMatch = data.match(RECOVERY_WORTH_REGEXP);
        if (scannedMatch && recoveredAmountMatch) {
          // JS probably doesn't care but rust would!
          let recoveredAmount = 0;
          if (recoveredAmountMatch[2] === "T") {
            // convert to micro tari
            recoveredAmount = Math.round(
              parseFloat(recoveredAmountMatch[1]) * 1000000
            );
          } else {
            recoveredAmount = parseInt(recoveredAmountMatch[1]);
          }
          return {
            numScanned: parseInt(scannedMatch[1]),
            recoveredAmount,
          };
        }

        return null;
      },
    });

    let endTime = new Date();
    const timeDiffMs = endTime - startTime;
    const timeDiffMinutes = timeDiffMs / 60000;

    let client = await wallet.connectClient();
    let id = await client.identify();

    await wallet.stop();

    const scannedRate = recoveryResult.numScanned / timeDiffMinutes;

    return {
      identity: id,
      numScanned: recoveryResult.numScanned,
      timeDiffMinutes,
      scannedRate: scannedRate.toFixed(2),
      recoveredAmount: recoveryResult.recoveredAmount,
    };
  } catch (err) {
    await wallet.stop();
    throw err;
  }
}

if (require.main === module) {
  Promise.all([main()]);
} else {
  module.exports = Object.assign(run, {
    RECOVERY_COMPLETE_REGEXP,
    RECOVERY_WORTH_REGEXP,
    FAILURE_REGEXP,
  });
}
