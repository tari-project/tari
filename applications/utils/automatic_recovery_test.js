const WalletProcess = require("../../integration_tests/helpers/walletProcess");
const WalletClient = require("../../integration_tests/helpers/walletClient");

const fs = require("fs/promises");
const yargs = require("yargs");
const path = require("path");

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
    .help()
    .alias("help", "h").argv;

  const wallet = new WalletProcess(
    "compile",
    true,
    {
      transport: "tor",
      network: "weatherwax",
      grpc_console_wallet_address: "127.0.0.1:18111",
    },
    false,
    argv.seedWords
  );

  await wallet.startNew();

  let startTime = new Date();

  await fs.mkdir(path.dirname(argv.log), { recursive: true });
  let logfile = await fs.open(argv.log, "w");

  let recoveryPromise = new Promise((resolve) => {
    wallet.ps.stderr.on("data", (data) => {
      logfile.write(data);
    });
    wallet.ps.stdout.on("data", (data) => {
      logfile.write(data);
      let height = data
        .toString()
        .match("Recovery\\ complete!\\ Scanned\\ =\\ (\\d+)\\ in");
      let recovered_ut = data.toString().match("worth\\ (\\d+)\\ µT");
      if (height && recovered_ut) {
        resolve([
          null,
          {
            height: parseInt(height[1]),
            recoveredAmount: parseInt(recovered_ut[1]),
          },
        ]);
      } else if (
        data
          .toString()
          .match("Attempt {}/{}: Failed to complete wallet recovery")
      ) {
        resolve([data, null]);
      }
    });
  });

  let [err, height_amount] = await recoveryPromise;

  let endTime = new Date();
  const timeDiffMs = endTime - startTime;
  const timeDiffMinutes = timeDiffMs / 60000;

  let walletClient = new WalletClient();
  await walletClient.connect("127.0.0.1:18111");
  let id = await walletClient.identify();

  wallet.stop();

  await fs.rmdir(__dirname + "/temp/base_nodes", { recursive: true });
  if (err) {
    console.log(`Wallet (Pubkey: ${id.public_key}) recovery failed`);
    console.log(`Error: ${err}`);
    process.exit(1);
    return;
  }

  const block_rate = height_amount.height / timeDiffMinutes;
  console.log(
    "Wallet (Pubkey:",
    id.public_key,
    ") recovered to a block height of",
    height_amount.height,
    "completed in",
    timeDiffMinutes.toFixed(2),
    "minutes (",
    block_rate.toFixed(2),
    "blocks/min).",
    height_amount.recoveredAmount,
    "µT recovered."
  );
}

Promise.all([main()]);
