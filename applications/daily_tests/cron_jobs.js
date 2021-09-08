const { CronJob } = require("cron");
const fs = require("fs/promises");
const {
  sendWebhookNotification,
  getWebhookUrl,
  readLastNLines,
  emptyFile,
} = require("./helpers");
const walletRecoveryTest = require("./automatic_recovery_test");
const baseNodeSyncTest = require("./automatic_sync_test");
const { SyncType } = require("./automatic_sync_test");

const WEBHOOK_CHANNEL = "protocol-bot-stuff";

function failed(message) {
  console.error(message);
  if (!!getWebhookUrl()) {
    sendWebhookNotification(WEBHOOK_CHANNEL, `🚨 ${message}`);
  }
  process.exit(1);
}

function notify(message) {
  console.log(message);
  if (!!getWebhookUrl()) {
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
          "spare man patrol essay divide hollow trip visual actress sadness country hungry toy blouse body club depend capital sleep aim high recycle crystal abandon",
        log: LOG_FILE,
        numWallets: instances,
      });

    notify(
      "🙌 Wallet (Pubkey:",
      identity.public_key,
      ") recovered to a block height of",
      numScanned,
      "completed in",
      timeDiffMinutes,
      "minutes (",
      scannedRate,
      "blocks/min).",
      recoveredAmount,
      "µT recovered for ",
      instances,
      " instance(s)."
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
  notify(`🚀 ${syncType} basenode sync check has begun 🚀`);

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

// ------------------------- CRON ------------------------- //
new CronJob("0 7 * * *", () => runWalletRecoveryTest(1)).start();
new CronJob("30 7 * * *", () => runWalletRecoveryTest(5)).start();
new CronJob("0 6 * * *", () => runBaseNodeSyncTest(SyncType.Archival)).start();
new CronJob("30 6 * * *", () => runBaseNodeSyncTest(SyncType.Pruned)).start();

console.log("Cron jobs started.");
