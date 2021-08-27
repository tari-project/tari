const fs = require("fs/promises");
const yargs = require("yargs");
const path = require("path");
const helpers = require("./helpers");
const BaseNodeProcess = require("integration_tests/helpers/baseNodeProcess");

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
      type: "number",
      description: "Enable force sync to peer_seeds i-th node.",
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
    const { blockHeight, timeDiffMinutes, blockRate, forcedSyncPeer } =
      await run(argv);

    console.log(
      `${argv.syncType} sync ${
        forcedSyncPeer ? "forced to " + forcedSyncPeer : ""
      } to block height ${blockHeight} took ${timeDiffMinutes} minutes for a rate of ${blockRate} blocks/min`
    );
  } catch (err) {
    console.log(err);
  }
}

async function run(options) {
  let forcedSyncPeer;
  const baseNode = new BaseNodeProcess("compile", true);
  await baseNode.init();

  // Set pruning horizon in config file if `pruned` command line arg is present
  if (options.syncType === SyncType.Pruned) {
    process.env.TARI_BASE_NODE__WEATHERWAX__PRUNING_HORIZON = 1000;
  }

  // Check if we have a forced peer index.
  if (options.forceSyncPeer !== undefined) {
    const config = (
      await fs.readFile(baseNode.baseDir + "/config/config.toml")
    ).toString();
    const peer = Array.from(
      config.match(/\npeer_seeds = \[(.*?)\]/s)[1].matchAll(/\n[^#]*"(.*?)"/g),
      (m) => m[1]
    )[options.forceSyncPeer];
    if (peer === undefined) {
      // Check if index is within bounds of peer_seeds from config.
      throw "Forced index out of bounds";
    }
    process.env.TARI_BASE_NODE__WEATHERWAX__FORCE_SYNC_PEERS = [peer];
    forcedSyncPeer = peer;
  }

  await baseNode.start();

  await fs.mkdir(path.dirname(options.log), { recursive: true });
  let logfile = await fs.open(options.log, "w+");
  let startTime = new Date();

  try {
    let syncResult = await helpers.monitorProcessOutput({
      process: baseNode.ps,
      outputStream: logfile,
      onDataCallback: (data) => {
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

    try {
      await fs.rmdir(__dirname + "/temp/base_nodes", { recursive: true });
    } catch (err) {
      console.error(err);
    }
    return {
      blockRate: blockRate.toFixed(2),
      timeDiffMinutes: timeDiffMinutes.toFixed(2),
      blockHeight: syncResult.height,
      forcedSyncPeer,
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
