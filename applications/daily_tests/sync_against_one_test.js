// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

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
    .option("nodes", {
      type: "number",
      description: "How many nodes should sync to one.",
      demandOption: true,
    })
    .option("log", {
      alias: "l",
      description: "output logs to this file",
      type: "string",
      default: "./logs/base-node.log",
    })
    .help()
    .alias("help", "h").argv;

  const { baseNode, height } = await start(argv);
  // At this point the baseNode is synced to the tip.
  // Now we force sync N nodes to this node and log the results.
  process.env.TARI_BASE_NODE__WEATHERWAX__FORCE_SYNC_PEERS = [
    baseNode.peerAddress(),
  ];
  // Set pruning horizon in config file if `pruned` command line arg is present
  if (argv.syncType === SyncType.Pruned) {
    process.env.TARI_BASE_NODE__WEATHERWAX__PRUNING_HORIZON = 1000;
  }

  const results = await Promise.all(
    [...Array(argv.nodes).keys()].map((i) => run(argv, height))
  );
  for (const { blockHeight, timeDiffMinutes, blockRate } of results) {
    console.log(
      `${argv.syncType} sync to block height ${blockHeight} took ${timeDiffMinutes} minutes for a rate of ${blockRate} blocks/min`
    );
  }
  await baseNode.stop();
  try {
    await fs.rm(__dirname + "/temp/base_nodes", { recursive: true });
  } catch (err) {
    console.error(err);
  }
}

// This start the first node, that others will use to sync to.
async function start(options) {
  process.env.TARI_BASE_NODE__WEATHERWAX__BASE_NODE_IDENTITY_FILE =
    "nodeid.json";
  const baseNode = new BaseNodeProcess("compile", true);
  await baseNode.init();

  await baseNode.start();

  await fs.mkdir(path.dirname(options.log), { recursive: true });
  let logfile = await fs.open(options.log, "w+");

  try {
    // When the connection break the node will think it's at the tip.
    // So we wait little bit to be sure we are at the tip.
    let listening_cnt = 0;
    const height = await helpers.monitorProcessOutput({
      process: baseNode.ps,
      outputStream: logfile,
      onDataCallback: (data) => {
        // 13:50 v0.9.3, State: Lisntening, Tip: 20515 (Wed, 18 Aug 2021 08:17:25 +0000), Mempool: 2tx (60g, +/- 1blks), Connections: 17, Banned: 0, Messages (last 60s): 36, Rpc: 3/1000 sessions
        let match = data.match(/Tip: (\d+) \(/);
        if (!match) {
          return null;
        }
        let height = parseInt(match[1]);

        if (height > 0 && data.toUpperCase().match(/STATE: LISTENING/)) {
          ++listening_cnt;
          if (listening_cnt == 3) return { height };
        } else {
          listening_cnt = 0;
        }
      },
    });
    return { baseNode, height };
  } catch (err) {
    await baseNode.stop();
    throw err;
  }
}

async function run(options, minimum_height) {
  const baseNode = new BaseNodeProcess("syncer", true);
  await baseNode.init();

  await baseNode.start();

  await fs.mkdir(path.dirname(options.log), { recursive: true });
  let logFile = await fs.open(options.log, "w+");
  let startTime = new Date();

  try {
    let syncResult = await helpers.monitorProcessOutput({
      process: baseNode.ps,
      outputStream: logFile,
      onDataCallback: (data) => {
        // 13:50 v0.9.3, State: Listening, Tip: 20515 (Wed, 18 Aug 2021 08:17:25 +0000), Mempool: 2tx (60g, +/- 1blks), Connections: 17, Banned: 0, Messages (last 60s): 36, Rpc: 3/1000 sessions
        let match = data.match(/Tip: (\d+) \(/);
        if (!match) {
          return null;
        }
        let height = parseInt(match[1]);

        if (height > 0 && data.toUpperCase().match(/STATE: LISTENING/)) {
          if (height >= minimum_height) return { height };
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
}
