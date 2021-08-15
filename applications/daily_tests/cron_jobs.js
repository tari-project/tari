const { CronJob } = require("cron");
const fs = require("fs/promises");
const {
  sendMattermostNotification,
  getMattermostWebhookUrl,
  readLastNLines,
  emptyFile,
} = require("./helpers");
const walletRecoveryTest = require("./automatic_recovery_test");
const baseNodeSyncTest = require("./automatic_sync_test");
const { SyncType } = require("./automatic_sync_test");

const MATTERMOST_CHANNEL = "protocol-bot-stuff";

function failed(message) {
  console.error(message);
  if (!!getMattermostWebhookUrl()) {
    sendMattermostNotification(MATTERMOST_CHANNEL, `🚨 ${message}`);
  }
  process.exit(1);
}

function notify(message) {
  console.log(message);
  if (!!getMattermostWebhookUrl()) {
    sendMattermostNotification(MATTERMOST_CHANNEL, message);
  }
}

async function runWalletRecoveryTest() {
  notify("🚀 Wallet recovery check has begun 🚀");

  const LOG_FILE = "./logs/wallet-recovery-test.log";
  await emptyFile(LOG_FILE);

  try {
    const { identity, timeDiffMinutes, height, blockRate, recoveredAmount } =
      await walletRecoveryTest({
        seedWords:
          "spare man patrol essay divide hollow trip visual actress sadness country hungry toy blouse body club depend capital sleep aim high recycle crystal abandon",
        log: LOG_FILE,
      });

    notify(
      "🙌 Wallet (Pubkey:",
      identity.public_key,
      ") recovered to a block height of",
      height,
      "completed in",
      timeDiffMinutes,
      "minutes (",
      blockRate,
      "blocks/min).",
      recoveredAmount,
      "µT recovered."
    );
  } catch (err) {
    console.error(err);
    let logLines = await readLastNLines(LOG_FILE, 15);
    failed(`Wallet recovery test failed.
Last log lines:
\`\`\`
${logLines.join("\n")}
\`\`\`
  `);
  }
}

async function runBaseNodeSyncTest(syncType) {
  notify(`🚀 ${syncType} basenode sync check has begun 🚀`);

  const LOG_FILE = `./logs/${syncType.toLowerCase()}-sync-test.log`;
  await emptyFile(LOG_FILE);

  try {
    const { blockHeight, timeDiffMinutes, blockRate } = await baseNodeSyncTest({
      log: LOG_FILE,
      syncType,
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
new CronJob("0 7 * * *", runWalletRecoveryTest).start();
new CronJob("0 6 * * *", () => runBaseNodeSyncTest(SyncType.Archival)).start();
new CronJob("30 6 * * *", () => runBaseNodeSyncTest(SyncType.Pruned)).start();

console.log("Cron jobs started.");
