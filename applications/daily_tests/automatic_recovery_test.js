const fs = require("fs/promises");
const yargs = require("yargs");
const path = require("path");
const helpers = require("./helpers");
const WalletProcess = require("integration_tests/helpers/walletProcess");
const WalletClient = require("integration_tests/helpers/walletClient");

async function main() {
  const argv = yargs
    .option("seed-words", {
      alias: "sw",
      description: "Seed words to use during recovery",
      type: "string",
      default:
        "pigeon marble letter canal hard close kit cash coin still melt random require long shaft antenna tent turkey neck divert enrich iron analyst abandon",
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
    let { identity, timeDiffMinutes, height, blockRate, recoveredAmount } =
      await run(argv);

    console.log(
      "Wallet (Pubkey:",
      identity.public_key,
      ") recovered to a block height of",
      height,
      "completed in",
      timeDiffMinutes,
      "minutes (",
      blockRate,
      "blocks/min).",
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
      network: "weatherwax",
      grpc_console_wallet_address: "127.0.0.1:18111",
    },
    false,
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
      onDataCallback: (data) => {
        let successLog = data.match(/Recovery complete! Scanned = (\d+) in/);
        let recoveredAmount = data.match(/worth ([0-9\.]+) (µ?T)/);
        if (successLog && recoveredAmount) {
          let recoveredAmount = parseInt(recoveredAmount[1]);
          if (recoveredAmount[2] === "T") {
            // convert to micro tari
            recoveredAmount *= 1000000;
          }
          return {
            height: parseInt(height[1]),
            recoveredAmount: parseInt(recoveredAmount[1]),
          };
        }

        let errMatch = data.match(
          /Attempt (\d+)\/(\d+): Failed to complete wallet recovery/
        );
        // One extra attempt
        if (errMatch && parseInt(errMatch[1]) > 1) {
          throw new Error(data);
        }

        return null;
      },
    });

    let endTime = new Date();
    const timeDiffMs = endTime - startTime;
    const timeDiffMinutes = timeDiffMs / 60000;

    let walletClient = new WalletClient();
    await walletClient.connect("127.0.0.1:18111");
    let id = await walletClient.identify();

    await wallet.stop();

    await fs.rmdir(__dirname + "/temp/base_nodes", { recursive: true });

    const block_rate = recoveryResult.height / timeDiffMinutes;

    return {
      identity: id,
      height: recoveryResult.height,
      timeDiffMinutes,
      blockRate: block_rate.toFixed(2),
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
  module.exports = run;
}
