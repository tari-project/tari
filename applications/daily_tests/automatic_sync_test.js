const fs = require("fs").promises;
const yargs = require("yargs");
const path = require("path");
const helpers = require("./helpers");
const BaseNodeProcess = require("integration_tests/helpers/baseNodeProcess");

const NETWORK = "WEATHERWAX";

const SyncType = {
  Archival: "Archival",
  Pruned: "Pruned",
};

async function main() {
  const argv = yargs
    .option("sync-type", {
      type: "string",
      description: "Specify the sync type.",
      default: SyncType.Archival,
      choices: [SyncType.Archival, SyncType.Pruned],
    })
    .option("force-sync-peer", {
      type: "string",
      description:
        "Pubkey and address string (formatted: `{pubkey}::{address}) of a the force sync peer.",
    })
    .option("log", {
      alias: "l",
      description: "output logs to this file",
      type: "string",
      default: "./logs/base-node.log",
    })
    .help()
    .alias("help", "h").argv;
  try {
    const options = {
      ...argv,
      forceSyncPeers: (argv.forceSyncPeers || "")
        .split(",")
        .map((s) => s.trim()),
    };
    const { blockHeight, timeDiffMinutes, blockRate } = await run(argv);

    const { forcedSyncPeer, syncType } = argv;
    console.log(
      `${syncType} sync ${
        forcedSyncPeer ? "forced to " + forcedSyncPeer : ""
      } to block height ${blockHeight} took ${timeDiffMinutes} minutes for a rate of ${blockRate} blocks/min`
    );
  } catch (err) {
    console.log(err);
  }
}

async function run(options) {
  const baseNode = new BaseNodeProcess("compile", true);
  await baseNode.init();

  // Bypass tor for outbound TCP (faster but less private)
  process.env[`TARI_BASE_NODE__${NETWORK}__TOR_PROXY_BYPASS_FOR_OUTBOUND_TCP`] =
    "true";
  // Set pruning horizon in config file if `pruned` command line arg is present
  if (options.syncType === SyncType.Pruned) {
    process.env[`TARI_BASE_NODE__${NETWORK}__PRUNING_HORIZON`] = 20;
  }

  if (options.forceSyncPeer) {
    let forcedSyncPeersStr = options.forceSyncPeer;
    if (Array.isArray(options.forceSyncPeer)) {
      forcedSyncPeersStr = options.forceSyncPeer.join(",");
    }
    console.log(`Force sync peer set to ${forcedSyncPeersStr}`);
    process.env[`TARI_BASE_NODE__${NETWORK}__FORCE_SYNC_PEERS`] =
      forcedSyncPeersStr;
  }

  await baseNode.start();

  await fs.mkdir(path.dirname(options.log), { recursive: true });
  let logfile = await fs.open(options.log, "w+");
  let startTime = new Date();

  try {
    let syncResult = await helpers.monitorProcessOutput({
      process: baseNode.ps,
      outputStream: logfile,
      onData: (data) => {
        // 13:50 v0.9.3, State: Listening, Tip: 20515 (Wed, 18 Aug 2021 08:17:25 +0000), Mempool: 2tx (60g, +/- 1blks), Connections: 17, Banned: 0, Messages (last 60s): 36, Rpc: 3/1000 sessions
        let match = data.match(/Tip: (\d+) \(/);
        if (!match) {
          return null;
        }
        let height = parseInt(match[1]);

        if (height > 0 && data.toUpperCase().match(/STATE: LISTENING/)) {
          return { height };
        }
      },
    });

    let endTime = new Date();
    const timeDiffMs = endTime - startTime;
    const timeDiffMinutes = timeDiffMs / 60000;
    const blockRate = syncResult.height / timeDiffMinutes;

    await baseNode.stop();

    return {
      blockRate: blockRate.toFixed(2),
      timeDiffMinutes: timeDiffMinutes.toFixed(2),
      blockHeight: syncResult.height,
    };
  } catch (err) {
    await baseNode.stop();
    throw err;
  }
}

if (require.main === module) {
  Promise.all([main()]);
} else {
  module.exports = run;
  Object.assign(module.exports, {
    SyncType,
  });
}
