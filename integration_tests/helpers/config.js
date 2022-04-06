// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

function mapEnvs(options) {
  const res = {};
  if (options.blocks_behind_before_considered_lagging) {
    res.TARI_BASE_NODE__LOCALNET__BLOCKS_BEHIND_BEFORE_CONSIDERED_LAGGING =
      options.blocks_behind_before_considered_lagging;
  }
  if (options.pruningHorizon) {
    // In the config toml file: `base_node.network.pruning_horizon` with `network = localnet`
    res.TARI_BASE_NODE__LOCALNET__PRUNING_HORIZON = options.pruningHorizon;
    res.TARI_BASE_NODE__LOCALNET__PRUNED_MODE_CLEANUP_INTERVAL = 1;
  }
  if ("num_confirmations" in options) {
    res.TARI_WALLET__TRANSACTION_NUM_CONFIRMATIONS_REQUIRED =
      options.num_confirmations;
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
  if ("mineOnTipOnly" in options) {
    res.TARI_MINING_NODE__MINE_ON_TIP_ONLY = options.mineOnTipOnly.toString();
  }
  if (options.numMiningThreads) {
    res.TARI_MINING_NODE__NUM_MINING_THREADS = options.numMiningThreads;
  }

  if (options.network) {
    res.TARI_BASE_NODE__NETWORK = options.network;
  }
  if (options.transport) {
    res.TARI_BASE_NODE__LOCALNET__TRANSPORT = options.transport;
    res.TARI_BASE_NODE__STIBBONS__TRANSPORT = options.transport;
  }
  if (options.common && options.common.auto_update) {
    let { auto_update } = options.common;
    if (auto_update.enabled) {
      res.TARI_COMMON__LOCALNET__AUTO_UPDATE__ENABLED = auto_update.enabled
        ? "true"
        : "false";
    }
    if (auto_update.check_interval) {
      res.TARI_COMMON__LOCALNET__AUTO_UPDATE__CHECK_INTERVAL =
        auto_update.check_interval;
    }
    if (auto_update.dns_hosts) {
      res.TARI_COMMON__LOCALNET__AUTO_UPDATE__DNS_HOSTS =
        auto_update.dns_hosts.join(",");
    }
    if (auto_update.hashes_url) {
      res.TARI_COMMON__LOCALNET__AUTO_UPDATE__HASHES_URL =
        auto_update.hashes_url;
    }
    if (auto_update.hashes_sig_url) {
      res.TARI_COMMON__LOCALNET__AUTO_UPDATE__HASHES_SIG_URL =
        auto_update.hashes_sig_url;
    }
  }
  return res;
}

function baseEnvs(peerSeeds = [], forceSyncPeers = [], committee = []) {
  const envs = {
    ["base_node.network"]: "localnet",
    ["wallet.network"]: "localnet",
    ["miner.network"]: "localnet",
    ["common.network"]: "localnet",
    ["common.config"]: "localnet",
    ["base_node.config"]: "localnet",
    ["wallet.config"]: "localnet",
    ["miner.config"]: "localnet",
    ["localnet.base_node.data_dir"]: "localnet",
    ["localnet.base_node.db_type"]: "lmdb",
    ["localnet.base_node.orphan_storage_capacity"]: "10",
    ["localnet.base_node.pruning_horizon"]: "0",
    ["localnet.base_node.pruned_mode_cleanup_interval"]: "10000",
    ["localnet.base_node.core_threads"]: "10",
    ["localnet.base_node.max_threads"]: "512",
    ["localnet.base_node.identity_file"]: "none.json",
    ["localnet.base_node.base_node_tor_identity_file"]: "torid.json",
    ["localnet.base_node.wallet_identity_file"]: "walletid.json",
    ["localnet.base_node.console_wallet_identity_file"]: "cwalletid.json",
    ["localnet.base_node.wallet_tor_identity_file"]: "wallettorid.json",
    ["localnet.base_node.console_wallet_tor_identity_file"]: "none.json",
    ["localnet.base_node.allow_test_addresses"]: true,
    ["base_node.grpc_enabled"]: true,
    ["localnet.base_node.enable_wallet"]: false,
    ["localnet.common.dns_seeds_use_dnssec"]: "false",
    ["localnet.common.dns_seeds"]: "",
    ["localnet.base_node.block_sync_strategy"]: "ViaBestChainMetadata",
    ["localnet.base_node.orphan_db_clean_out_threshold"]: "0",
    ["localnet.base_node.max_randomx_vms"]: "1",
    ["localnet.base_node.auto_ping_interval"]: "15",
    ["localnet.wallet.contacts_auto_ping_interval"]: "5",
    ["localnet.base_node.flood_ban_max_msg_count"]: "100000",
    ["localnet.merge_mining_proxy.monerod_url"]: [
      "http://stagenet.xmr-tw.org:38081",
      "http://stagenet.community.xmr.to:38081",
      "http://monero-stagenet.exan.tech:38081",
      "http://xmr-lux.boldsuck.org:38081",
      "http://singapore.node.xmr.pm:38081",
    ],
    ["merge_mining_proxy.monerod_use_auth"]: false,
    ["merge_mining_proxy.monerod_username"]: '""',
    ["merge_mining_proxy.monerod_password"]: '""',
    ["localnet.base_node.db_init_size_mb"]: 100,
    ["localnet.base_node.db_resize_threshold_mb"]: 10,
    ["localnet.base_node.db_grow_size_mb"]: 20,
    ["merge_mining_proxy.wait_for_initial_sync_at_startup"]: false,
    ["mining_node.num_mining_threads"]: "1",
    ["mining_node.mine_on_tip_only"]: true,
    ["mining_node.validate_tip_timeout_sec"]: 1,
    ["wallet.grpc_enabled"]: true,
    ["wallet.scan_for_utxo_interval"]: 5,
  };
  if (forceSyncPeers.length > 0) {
    envs["localnet.base_node.force_sync_peers"] = forceSyncPeers.join(",");
  }
  if (peerSeeds.length > 0) {
    envs["localnet.common.peer_seeds"] = peerSeeds.join(",");
  }
  if (committee.length !== 0) {
    envs["localnet.dan_node.committee "] = committee;
  }

  return envs;
}

let defaultArgs = {
  isWallet: false,
  nodeFile: "newnodeid.json",
  walletGrpcAddress: "/ip4/127.0.0.1/tcp/8082",
  baseNodeGrpcAddress: "/ip4/127.0.0.1/tcp/8080",
  walletPort: 8083,
  baseNodePort: 8081,
  proxyFullAddress: "127.0.0.1:8084",
  transcoderFullAddress: "127.0.0.1:8085",
  options: {},
  peerSeeds: [],
  forceSyncPeers: [],
  committee: [],
};

function createEnv(args) {
  let {
    isWallet,
    nodeFile,
    walletGrpcAddress,
    walletPort,
    baseNodeGrpcAddress,
    baseNodePort,
    proxyFullAddress,
    options,
    peerSeeds,
    forceSyncPeers,
    committee,
  } = { ...defaultArgs, ...args };

  const envs = baseEnvs(peerSeeds, forceSyncPeers, committee);
  const network =
    options && options.network ? options.network.toLowerCase() : "localnet";
  const configEnvs = {
    [`base_node.grpc_enabledddd`]: `true`,
    [`base_node.grpc_address`]: baseNodeGrpcAddress,
    [`${network}.base_node.identity_file`]: `${nodeFile}`,
    [`${network}.base_node.p2p.transport.type`]: "tcp",
    [`${network}.base_node.p2p.transport.tcp.listener_address`]:
      "/ip4/127.0.0.1/tcp/" + (isWallet ? `${walletPort}` : `${baseNodePort}`),
    [`${network}.base_node.p2p.public_address`]:
      "/ip4/127.0.0.1/tcp/" + (isWallet ? `${walletPort}` : `${baseNodePort}`),

    [`wallet.grpc_address`]: walletGrpcAddress,
    [`${network}.wallet.p2p.transport.type`]: "tcp",
    [`${network}.wallet.p2p.transport.tcp.listener_address`]: `/ip4/127.0.0.1/tcp/${walletPort}`,
    [`${network}.wallet.p2p.public_address`]: `/ip4/127.0.0.1/tcp/${walletPort}`,

    [`merge_mining_proxy.listener_address`]: `${proxyFullAddress}`,
    [`${network}.merge_mining_proxy.grpc_base_node_address`]:
      baseNodeGrpcAddress,
    [`${network}.merge_mining_proxy.grpc_console_wallet_address`]:
      walletGrpcAddress,
  };

  return { ...envs, ...configEnvs, ...mapEnvs(options || {}) };
}

module.exports = {
  createEnv,
};
