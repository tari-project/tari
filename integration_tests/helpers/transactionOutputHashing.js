// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const { consensusHashers } = require("./hashing");
const {
  toLittleEndian,
  encodeOption,
  toLengthEncoded,
  assertBufferType,
} = require("./util");

const featuresToConsensusBytes = function (features) {
  // base_layer\core\src\transactions\transaction\output_features.rs (fn consensus_encode)

  // TODO: Keep this number in sync with 'get_current_version()' in 'output_features_version.rs'
  const OUTPUT_FEATURES_VERSION = 0x00;

  return Buffer.concat([
    // version
    Buffer.from([OUTPUT_FEATURES_VERSION]),
    // maturity
    Buffer.from([parseInt(features.maturity || 0)]),
    // output_type
    Buffer.from([features.output_type]),
    // sidechain_features
    // TODO: SideChainFeatures
    encodeOption(null),
    // metadata
    // TODO: Vec<u8> (len is 0)
    Buffer.from([0x00]),
  ]);
};

const getTransactionOutputHash = function (output) {
  // base_layer\core\src\transactions\transaction_components\mod.rs (fn hash_output)

  // TODO: Keep this number in sync with 'get_current_version()' in 'transaction_output_version.rs'
  const OUTPUT_FEATURES_VERSION = 0x00;

  assertBufferType(output.commitment, 32);
  assertBufferType(output.script);
  assertBufferType(output.covenant);
  assertBufferType(output.encrypted_value, 24);
  assertBufferType(output.sender_offset_public_key, 32);
  const hash = consensusHashers
    .transactionHasher("transaction_output")
    // version
    .chain(Buffer.from([OUTPUT_FEATURES_VERSION]))
    // features
    .chain(featuresToConsensusBytes(output.features))
    // commitment
    .chain(output.commitment)
    // script
    .chain(toLengthEncoded(output.script))
    // covenant
    .chain(toLengthEncoded(output.covenant))
    // encrypted_value
    .chain(output.encrypted_value)
    // sender offset public key
    .chain(output.sender_offset_public_key)
    // minimum_value_promise
    .chain(toLittleEndian(output.minimum_value_promise, 64))
    .finalize();

  const hashBuffer = Buffer.from(hash);
  // console.log(
  //   "\ngetTransactionOutputHash - hash",
  //   hashBuffer.toString("hex"),
  //   "\n"
  // );
  return hashBuffer;
};

module.exports = { getTransactionOutputHash, featuresToConsensusBytes };
