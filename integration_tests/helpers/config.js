const { spawnSync, spawn, execSync } = require("child_process");
const { expect } = require("chai");
const fs = require("fs");
const { getFreePort } = require("./util");
const dateFormat = require("dateformat");

function mapEnvs(options) {
  let res = {};
  if (options.blocks_behind_before_considered_lagging) {
    res.TARI_BASE_NODE__LOCALNET__BLOCKS_BEHIND_BEFORE_CONSIDERED_LAGGING =
      options.blocks_behind_before_considered_lagging;
  }
  if (options.pruningHorizon) {
    // In the config toml file: `base_node.network.pruning_horizon` with `network = localnet`
    res.TARI_BASE_NODE__LOCALNET__PRUNING_HORIZON = options.pruningHorizon;
  }
  if (options.routingMechanism) {
    // In the config toml file: `wallet.transaction_routing_mechanism`
    res.TARI_WALLET__TRANSACTION_ROUTING_MECHANISM = options.routingMechanism;
  }
  if (options.broadcastMonitoringTimeout) {
    res.TARI_WALLET__TRANSACTION_BROADCAST_MONITORING_TIMEOUT =
      options.broadcastMonitoringTimeout;
  } else {
    res.TARI_WALLET__TRANSACTION_BROADCAST_MONITORING_TIMEOUT = 3;
  }
  return res;
}

function baseEnvs(peerSeeds = []) {
  let envs = {
    RUST_BACKTRACE: 1,
    TARI_BASE_NODE__NETWORK: "localnet",
    TARI_BASE_NODE__LOCALNET__DATA_DIR: "localnet",
    TARI_BASE_NODE__LOCALNET__DB_TYPE: "lmdb",
    TARI_BASE_NODE__LOCALNET__ORPHAN_STORAGE_CAPACITY: "10",
    TARI_BASE_NODE__LOCALNET__PRUNING_HORIZON: "0",
    TARI_BASE_NODE__LOCALNET__PRUNED_MODE_CLEANUP_INTERVAL: "10000",
    TARI_BASE_NODE__LOCALNET__CORE_THREADS: "10",
    TARI_BASE_NODE__LOCALNET__MAX_THREADS: "512",
    TARI_BASE_NODE__LOCALNET__IDENTITY_FILE: "none.json",
    TARI_BASE_NODE__LOCALNET__BASE_NODE_TOR_IDENTITY_FILE: "torid.json",
    TARI_BASE_NODE__LOCALNET__WALLET_IDENTITY_FILE: "walletid.json",
    TARI_BASE_NODE__LOCALNET__CONSOLE_WALLET_IDENTITY_FILE: "cwalletid.json",
    TARI_BASE_NODE__LOCALNET__WALLET_TOR_IDENTITY_FILE: "wallettorid.json",
    TARI_BASE_NODE__LOCALNET__CONSOLE_WALLET_TOR_IDENTITY_FILE: "none.json",
    TARI_BASE_NODE__LOCALNET__ALLOW_TEST_ADDRESSES: true,
    TARI_BASE_NODE__LOCALNET__GRPC_ENABLED: true,
    TARI_BASE_NODE__LOCALNET__ENABLE_WALLET: false,
    TARI_BASE_NODE__LOCALNET__DNS_SEEDS_NAME_SERVER: "1.1.1.1:53",
    TARI_BASE_NODE__LOCALNET__DNS_SEEDS_USE_DNSSEC: "true",
    TARI_BASE_NODE__LOCALNET__BLOCK_SYNC_STRATEGY: "ViaBestChainMetadata",
    TARI_BASE_NODE__LOCALNET__NUM_MINING_THREADS: "1",
    TARI_BASE_NODE__LOCALNET__ORPHAN_DB_CLEAN_OUT_THRESHOLD: "0",
    TARI_BASE_NODE__LOCALNET__MAX_RANDOMX_VMS: "1",
    TARI_BASE_NODE__LOCALNET__AUTO_PING_INTERVAL: "15",
    TARI_BASE_NODE__LOCALNET__FLOOD_BAN_MAX_MSG_COUNT: "100000",
    TARI_MERGE_MINING_PROXY__LOCALNET__MONEROD_URL:
      "http://18.133.55.120:38081",
    TARI_MERGE_MINING_PROXY__LOCALNET__MONEROD_USE_AUTH: false,
    TARI_MERGE_MINING_PROXY__LOCALNET__MONEROD_USERNAME: '""',
    TARI_MERGE_MINING_PROXY__LOCALNET__MONEROD_PASSWORD: '""',
    TARI_BASE_NODE__LOCALNET__DB_INIT_SIZE_MB: 100,
    TARI_BASE_NODE__LOCALNET__DB_RESIZE_THRESHOLD_MB: 10,
    TARI_BASE_NODE__LOCALNET__DB_GROW_SIZE_MB: 20,
    TARI_MERGE_MINING_PROXY__LOCALNET__WAIT_FOR_INITIAL_SYNC_AT_STARTUP: false,
  };
  if (peerSeeds.length != 0) {
    envs.TARI_BASE_NODE__LOCALNET__PEER_SEEDS = peerSeeds;
  } else {
    //  Nowheresville
    envs.TARI_BASE_NODE__LOCALNET__PEER_SEEDS = [
      "5cfcf17f41b01980eb4fa03cec5ea12edbd3783496a2b5dabf99e4bf6410f460::/ip4/10.0.0.50/tcp/1",
    ];
  }

  return envs;
}

function createEnv(
  name = "config_identity",
  isWallet = false,
  nodeFile = "newnodeid.json",
  walletGrpcAddress = "127.0.0.1",
  walletGrpcPort = "8082",
  walletPort = "8083",
  baseNodeGrpcAddress = "127.0.0.1",
  baseNodeGrpcPort = "8080",
  baseNodePort = "8081",
  proxyFullAddress = "127.0.0.1:8084",
  options,
  peerSeeds = [],
  txnSendingMechanism = "DirectAndStoreAndForward"
) {
  var envs = baseEnvs(peerSeeds);
  var configEnvs = {
    TARI_BASE_NODE__LOCALNET__GRPC_BASE_NODE_ADDRESS: `${baseNodeGrpcAddress}:${baseNodeGrpcPort}`,
    TARI_BASE_NODE__LOCALNET__GRPC_CONSOLE_WALLET_ADDRESS: `${walletGrpcAddress}:${walletGrpcPort}`,
    TARI_BASE_NODE__LOCALNET__BASE_NODE_IDENTITY_FILE: `${nodeFile}`,
    TARI_BASE_NODE__LOCALNET__TCP_LISTENER_ADDRESS:
      "/ip4/0.0.0.0/tcp/" + (isWallet ? `${walletPort}` : `${baseNodePort}`),
    TARI_BASE_NODE__LOCALNET__PUBLIC_ADDRESS:
      "/ip4/127.0.0.1/tcp/" + (isWallet ? `${walletPort}` : `${baseNodePort}`),
    TARI_MERGE_MINING_PROXY__LOCALNET__PROXY_HOST_ADDRESS: `${proxyFullAddress}`,
    TARI_BASE_NODE__LOCALNET__TRANSPORT: "tcp",
  };
  //console.log(configEnvs);
  var fullEnvs = { ...envs, ...configEnvs };
  //if (isWallet) {
  //console.log(name, mapEnvs(options || {}));
  //}
  return { ...fullEnvs, ...mapEnvs(options || {}) };
}

module.exports = {
  createEnv,
};
