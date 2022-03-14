const { CronJob } = require("cron");
const { promises: fs } = require("fs");
const {
  sendWebhookNotification,
  getWebhookUrl,
  readLastNLines,
  emptyFile,
  git,
} = require("./helpers");
const walletRecoveryTest = require("./automatic_recovery_test");
const baseNodeSyncTest = require("./automatic_sync_test");
const { SyncType } = require("./automatic_sync_test");

const WEBHOOK_CHANNEL = "protocol-bot-stuff";

function failed(message) {
  console.error(message);
  if (getWebhookUrl()) {
    sendWebhookNotification(WEBHOOK_CHANNEL, `🚨 ${message}`);
  }
  process.exit(1);
}

function notify(message) {
  console.log(message);
  if (getWebhookUrl()) {
    sendWebhookNotification(WEBHOOK_CHANNEL, message);
  }
}

async function runWalletRecoveryTest(instances) {
  notify("🚀 Wallet recovery check has begun 🚀");

  const baseDir = __dirname + "/temp/wallet-recovery";
  // Remove the last run data
  try {
    await fs.rmdir(baseDir, {
      recursive: true,
    });
  } catch (err) {
    console.error(err);
  }

  const LOG_FILE = "./logs/wallet-recovery-test.log";
  await emptyFile(LOG_FILE);

  try {
    const {
      identity,
      timeDiffMinutes,
      numScanned,
      scannedRate,
      recoveredAmount,
    } = await walletRecoveryTest({
      seedWords:
        "cactus pool fuel skull chair casino season disorder flat crash wrist whisper decorate narrow oxygen remember minor among happy cricket embark blue ship sick",
      log: LOG_FILE,
      numWallets: instances,
      baseDir,
    });

    notify(
      `🙌 Wallet (Pubkey: ${identity.public_key} ) recovered scanned ${numScanned} UTXO's, completed in ${timeDiffMinutes} minutes (${scannedRate} UTXOs/min). ${recoveredAmount} µT recovered for ${instances} instance(s).`
    );
  } catch (err) {
    console.error(err);
    let logLines = await readLastNLines(LOG_FILE, 15);
    failed(`Wallet recovery test failed.
    ${err}
Last log lines:
\`\`\`
${logLines.join("\n")}
\`\`\`
  `);
  }
}

async function runBaseNodeSyncTest(syncType) {
  const node_pub_key =
    "b0c1f788f137ba0cdc0b61e89ee43b80ebf5cca4136d3229561bf11eba347849";
  const node_address = "/ip4/3.8.193.254/tcp/18189";
  notify(
    `🚀 ${syncType} basenode sync (from ${node_pub_key}) check has begun 🚀`
  );

  const baseDir = __dirname + "/temp/base-node-sync";

  // Remove the last run data
  try {
    await fs.rmdir(baseDir, {
      recursive: true,
    });
  } catch (err) {
    console.error(err);
  }

  const LOG_FILE = `./logs/${syncType.toLowerCase()}-sync-test.log`;
  await emptyFile(LOG_FILE);

  try {
    const { blockHeight, timeDiffMinutes, blockRate } = await baseNodeSyncTest({
      log: LOG_FILE,
      syncType,
      baseDir,
      forceSyncPeers: [`${node_pub_key}::${node_address}`],
    });

    notify(
      `⛓ Syncing ${syncType} to block height ${blockHeight} took ${timeDiffMinutes} minutes for a rate of ${blockRate} blocks/min`
    );
  } catch (err) {
    console.error(err);
    let logLines = await readLastNLines(LOG_FILE, 15);
    failed(`Base node ${syncType} sync test failed. 
${err}
Last log lines:
\`\`\`
${logLines.join("\n")}
\`\`\`
  `);
  }
}

async function main() {
  console.log("👩‍💻 Updating repo...");
  await git.reset(__dirname).catch((err) => {
    console.error("🚨 Failed to do git reset --hard");
    console.error(err);
  });
  await git.pull(__dirname).catch((err) => {
    console.error("🚨 Failed to update git repo");
    console.error(err);
  });

  // ------------------------- CRON ------------------------- //
  new CronJob("0 2 * * *", () => runWalletRecoveryTest(1)).start();
  //new CronJob("30 7 * * *", () => runWalletRecoveryTest(5)).start();
  new CronJob("0 1 * * *", () =>
    runBaseNodeSyncTest(SyncType.Archival)
  ).start();
  new CronJob("30 1 * * *", () => runBaseNodeSyncTest(SyncType.Pruned)).start();
  new CronJob("0 0 * * *", () =>
    git
      .reset(__dirname)
      .catch((err) => {
        console.error("🚨 Failed to do git reset --hard");
        console.error(err);
        return Promise.resolve(null);
      })
      .then(
        git.pull(__dirname).catch((err) => {
          failed("Failed to update git repo");
          console.error(err);
          return Promise.resolve(null);
        })
      )
  ).start();

  console.log("⏱ Cron jobs started.");
}

Promise.all([main()]);
