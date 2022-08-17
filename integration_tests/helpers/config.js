// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

function mapEnvs(options) {
  const res = {};
  if (options.blocks_behind_before_considered_lagging) {
    res["localnet.base_node.blocks_behind_before_considered_lagging"] =
      options.blocks_behind_before_considered_lagging;
  }
  if (options.pruningHorizon) {
    // In the config toml file: `base_node.network.pruning_horizon` with `network = localnet`
    res["localnet.base_node.storage.pruning_horizon"] = options.pruningHorizon;
    res["localnet.base_node.storage.pruning_interval"] = "1";
  }
  if ("numConfirmations" in options) {
    res["wallet.num_required_confirmations"] = options.numConfirmations;
    res["wallet.transactions.num_confirmations_required"] =
      options.numConfirmations;
    res["wallet.outputs.num_confirmations_required"] = options.numConfirmations;
  }
  if ("num_confirmations" in options) {
    res["wallet.num_required_confirmations"] = options.num_confirmations;
    res["wallet.transactions.num_confirmations_required"] =
      options.num_confirmations;
    res["wallet.outputs.num_confirmations_required"] =
      options.num_confirmations;
  }
  if (options.routingMechanism) {
    res["wallet.transactions.transaction_routing_mechanism"] =
      options.routingMechanism;
  }
  if (options.broadcastMonitoringTimeout) {
    res["wallet.transactions.broadcast_monitoring_timeout"] =
      options.broadcastMonitoringTimeout;
  } else {
    res["wallet.transactions.broadcast_monitoring_timeout"] = "3";
  }
  if (options.numMiningThreads) {
    res["miner.num_mining_threads"] = options.numMiningThreads;
  }

  if (options.network) {
    res["base_node.network"] = options.network;
  }
  if (options.transport) {
    res["localnet.base_node.transport"] = options.transport;
  }
  if (options.common && options.common.auto_update) {
    let { auto_update } = options.common;
    if (auto_update.enabled) {
      res["localnet.auto_update.enabled"] = auto_update.enabled
        ? "true"
        : "false";
    }
    if (auto_update.check_interval) {
      res["localnet.auto_update.check_interval"] = auto_update.check_interval;
    }
    if (auto_update.dns_hosts) {
      res["localnet.auto_update.dns_hosts"] = auto_update.dns_hosts.join(",");
    }
    if (auto_update.hashes_url) {
      res["localnet.auto_update.hashes_url"] = auto_update.hashes_url;
    }
    if (auto_update.hashes_sig_url) {
      res["localnet.auto_update.hashes_sig_url"] = auto_update.hashes_sig_url;
    }
  }
  return res;
}

function baseEnvs(peerSeeds = [], forceSyncPeers = [], _committee = []) {
  const envs = {
    ["base_node.network"]: "localnet",
    ["wallet.network"]: "localnet",
    ["localnet.base_node.data_dir"]: "localnet",
    ["localnet.base_node.db_type"]: "lmdb",
    ["localnet.base_node.storage.orphan_storage_capacity"]: "10",
    ["localnet.base_node.storage.pruning_horizon"]: "0",
    ["localnet.base_node.identity_file"]: "none.json",
    ["localnet.base_node.tor_identity_file"]: "torid.json",
    ["localnet.base_node.max_randomx_vms"]: "1",
    ["localnet.base_node.metadata_auto_ping_interval"]: "15",
    ["localnet.base_node.p2p.allow_test_addresses"]: true,
    ["localnet.base_node.p2p.dht.flood_ban_max_msg_count"]: "100000",
    ["localnet.base_node.p2p.dht.database_url"]: "localnet/dht.db",
    ["localnet.p2p.seeds.dns_seeds_use_dnssec"]: "false",

    ["localnet.wallet.identity_file"]: "walletid.json",
    ["localnet.wallet.contacts_auto_ping_interval"]: "5",
    ["localnet.wallet.p2p.allow_test_addresses"]: true,
    ["localnet.wallet.p2p.dht.flood_ban_max_msg_count"]: "100000",
    ["localnet.wallet.p2p.dht.saf.auto_request"]: true,

    ["localnet.merge_mining_proxy.monerod_url"]: [
      "http://stagenet.xmr-tw.org:38081",
      "http://stagenet.community.xmr.to:38081",
      "http://monero-stagenet.exan.tech:38081",
      "http://xmr-lux.boldsuck.org:38081",
      "http://singapore.node.xmr.pm:38081",
    ].join(","),
    ["merge_mining_proxy.monerod_use_auth"]: false,
    ["merge_mining_proxy.monerod_username"]: "",
    ["merge_mining_proxy.monerod_password"]: "",
    // ["localnet.base_node.storage_db_init_size"]: 100000000,
    // ["localnet.base_node.storage.db_resize_threshold"]: 10000000,
    // ["localnet.base_node.storage.db_grow_size"]: 20000000,
    ["merge_mining_proxy.wait_for_initial_sync_at_startup"]: false,
    ["miner.num_mining_threads"]: "1",
    ["miner.mine_on_tip_only"]: true,
    ["miner.validate_tip_timeout_sec"]: "1",
  };
  if (forceSyncPeers.length > 0) {
    envs["localnet.base_node.force_sync_peers"] = forceSyncPeers.join(",");
  }
  if (peerSeeds.length > 0) {
    envs["localnet.p2p.seeds.peer_seeds"] = peerSeeds.join(",");
  }

  return envs;
}

let defaultOpts = {
  isWallet: false,
  nodeFile: "newnodeid.json",
  walletGrpcAddress: "/ip4/127.0.0.1/tcp/19082",
  baseNodeGrpcAddress: "/ip4/127.0.0.1/tcp/19080",
  walletPort: 8083,
  baseNodePort: 8081,
  proxyFullAddress: "127.0.0.1:8084",
  transcoderFullAddress: "127.0.0.1:8085",
  options: {},
  peerSeeds: [],
  forceSyncPeers: [],
  committee: [],
};

// TODO: We can split these per application now
function createEnv(opts) {
  const finalOpts = { ...defaultOpts, ...opts };
  let {
    nodeFile,
    walletGrpcAddress,
    walletPort,
    baseNodeGrpcAddress,
    baseNodePort,
    network = "localnet",
    proxyFullAddress,
    peerSeeds,
    forceSyncPeers,
    options,
    committee,
    validatorNodeGrpcAddress,
  } = finalOpts;

  const envs = baseEnvs(peerSeeds, forceSyncPeers, committee);
  const configEnvs = {
    [`${network}.base_node.grpc_address`]: baseNodeGrpcAddress,
    [`${network}.base_node.identity_file`]: `${nodeFile}`,
    [`${network}.base_node.p2p.transport.type`]: "tcp",
    [`${network}.base_node.p2p.transport.tcp.listener_address`]: `/ip4/127.0.0.1/tcp/${baseNodePort}`,
    [`${network}.base_node.p2p.public_address`]: `/ip4/127.0.0.1/tcp/${baseNodePort}`,
    ["base_node.report_grpc_error"]: true,

    [`wallet.grpc_address`]: walletGrpcAddress,
    [`${network}.wallet.grpc_address`]: walletGrpcAddress,
    [`${network}.wallet.p2p.transport.type`]: "tcp",
    [`${network}.wallet.p2p.transport.tcp.listener_address`]: `/ip4/127.0.0.1/tcp/${walletPort}`,
    [`${network}.wallet.p2p.public_address`]: `/ip4/127.0.0.1/tcp/${walletPort}`,

    [`merge_mining_proxy.listener_address`]: proxyFullAddress,
    [`${network}.merge_mining_proxy.base_node_grpc_address`]:
      baseNodeGrpcAddress,
    [`${network}.merge_mining_proxy.console_wallet_grpc_address`]:
      walletGrpcAddress,

    [`miner.base_node_grpc_address`]: baseNodeGrpcAddress,
    [`miner.wallet_grpc_address`]: walletGrpcAddress,

    [`validator_node.base_node_grpc_address`]: baseNodeGrpcAddress,
    [`validator_node.wallet_grpc_address`]: walletGrpcAddress,
    [`validator_node.p2p.transport.type`]: "tcp",
    [`validator_node.grpc_address`]: validatorNodeGrpcAddress,
  };
  let finalEnv = { ...envs, ...configEnvs, ...mapEnvs(options || {}) };
  return finalEnv;
}

module.exports = {
  createEnv,
};
