// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const { Blake256 } = require("./hashing");
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

  // Add length byte to unique id - note this only works until 127 bytes (TODO: varint encoding)
  let unique_id = features.unique_id
    ? toLengthEncoded(features.unique_id)
    : null;

  return Buffer.concat([
    // version
    Buffer.from([OUTPUT_FEATURES_VERSION]),
    // maturity
    Buffer.from([parseInt(features.maturity || 0)]),
    // output_type
    Buffer.from([features.output_type]),
    // parent_public_key
    encodeOption(features.parent_public_key, "hex"),
    // unique_id
    encodeOption(unique_id, false),
    // sidechain_features
    // TODO: SideChainFeatures
    encodeOption(null),
    // asset
    // TODO: AssetOutputFeatures
    encodeOption(null),
    // mint_non_fungible
    // TODO: MintNonFungibleFeatures
    encodeOption(null),
    // sidechain_checkpoint
    // TODO: SideChainCheckpointFeatures
    encodeOption(null),
    // metadata
    // TODO: Vec<u8> (len is 0)
    Buffer.from([0x00]),
    // committee_definition
    // TODO: CommitteeDefinitionFeatures (len is 0)
    encodeOption(null),
  ]);
};

const getTransactionOutputHash = function (output) {
  // base_layer\core\src\transactions\transaction_components\mod.rs (fn hash_output)

  // TODO: Keep this number in sync with 'get_current_version()' in 'transaction_output_version.rs'
  const OUTPUT_FEATURES_VERSION = 0x00;

  let hasher = new Blake256();
  assertBufferType(output.commitment, 32);
  assertBufferType(output.script);
  assertBufferType(output.covenant);
  assertBufferType(output.encrypted_value, 24);
  const hash = hasher
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
