// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const tari_crypto = require("tari_crypto");
const {
  toLittleEndian,
  littleEndianHexStringToBigEndianHexString,
  combineTwoTariKeys,
  assertBufferType,
  varintEncode,
} = require("../helpers/util");
const { featuresToConsensusBytes } = require("./transactionOutputHashing");
const { consensusHashers } = require("./hashing");
const { OutputType } = require("./types");

class TransactionBuilder {
  constructor() {
    this.kv = tari_crypto.KeyRing.new();
    this.inputs = [];
    this.outputs = [];
    this.fee = 100;
    this.lockHeight = 0;
  }

  generatePrivateKey(id) {
    this.kv.new_key(id);
    return this.kv.private_key(id);
  }

  buildKernelChallenge(publicNonce, publicExcess, fee, lockHeight, features) {
    const option_none = Buffer.from("00", "hex");
    let hash = consensusHashers
      .transactionHasher("kernel_signature")
      .chain(publicNonce, "hex")
      .chain(publicExcess, "hex")
      .chain(toLittleEndian(fee, 64))
      .chain(varintEncode(lockHeight))
      .chain(toLittleEndian(features, 8))
      .chain(option_none)
      .finalize();

    return Buffer.from(hash).toString("hex");
  }

  toLengthEncoded(buf) {
    return Buffer.concat([varintEncode(buf.length), buf]);
  }

  // Everything passed into this function must be a byte buffer except for features, minimumValuePromise
  buildMetaChallenge(
    script,
    features,
    scriptOffsetPublicKey,
    publicNonce,
    commitment,
    covenant,
    encryptedValue,
    minimumValuePromise
  ) {
    assertBufferType(publicNonce, 32);
    assertBufferType(script);
    assertBufferType(scriptOffsetPublicKey, 32);
    assertBufferType(commitment, 32);
    assertBufferType(covenant);
    assertBufferType(encryptedValue);

    // base_layer/core/src/transactions/transaction/transaction_output.rs
    let hash = consensusHashers
      .transactionHasher("metadata_signature")
      .chain(publicNonce)
      .chain(this.toLengthEncoded(script))
      .chain(featuresToConsensusBytes(features))
      .chain(scriptOffsetPublicKey)
      .chain(commitment)
      .chain(this.toLengthEncoded(covenant))
      .chain(encryptedValue)
      .chain(toLittleEndian(minimumValuePromise, 64))
      .finalize();

    return Buffer.from(hash);
  }

  buildScriptChallenge(
    publicNonce,
    script,
    input_data,
    public_key,
    commitment
  ) {
    assertBufferType(publicNonce);
    assertBufferType(script);
    assertBufferType(input_data);
    assertBufferType(public_key, 32);
    assertBufferType(commitment, 32);
    let hash = consensusHashers
      .transactionHasher("script_challenge")
      .chain(publicNonce)
      .chain(this.toLengthEncoded(script))
      .chain(this.toLengthEncoded(input_data))
      .chain(public_key)
      .chain(commitment)
      .finalize();

    return Buffer.from(hash);
  }

  changeFee(fee) {
    this.fee = fee;
  }

  addInput(input) {
    let nopScriptBytes = Buffer.from([0x73]);
    let scriptPublicKeyHex = tari_crypto.pubkey_from_secret(
      input.scriptPrivateKey.toString("hex")
    );
    let scriptPublicKey = Buffer.from(scriptPublicKeyHex, "hex");
    // The 0x04 is type code for a pubkey in TariScript
    let input_data = Buffer.concat([Buffer.from([0x04]), scriptPublicKey]);
    this.kv.new_key("common_nonce_1");
    this.kv.new_key("common_nonce_2");
    let private_nonce_1 = this.kv.private_key("common_nonce_1");
    let private_nonce_2 = this.kv.private_key("common_nonce_2");
    let public_nonce_hex = tari_crypto.commit_private_keys(
      private_nonce_1,
      private_nonce_2
    ).commitment;
    let public_nonce = Buffer.from(public_nonce_hex, "hex");

    let commitment = input.output.commitment;
    let challenge = this.buildScriptChallenge(
      public_nonce,
      nopScriptBytes,
      input_data,
      scriptPublicKey,
      commitment
    );
    let amount_key = Buffer.from(toLittleEndian(input.amount, 256)).toString(
      "hex"
    );
    let total_key = combineTwoTariKeys(
      input.scriptPrivateKey.toString(),
      input.privateKey.toString()
    );

    let script_sig = tari_crypto.sign_comsig_challenge_with_nonce(
      amount_key,
      total_key,
      private_nonce_1,
      private_nonce_2,
      challenge.toString("hex")
    );
    this.inputs.push({
      input: {
        features: input.output.features,
        commitment: input.output.commitment,
        script: nopScriptBytes,
        input_data: input_data,
        height: 0,
        script_signature: {
          public_nonce_commitment: Buffer.from(script_sig.public_nonce, "hex"),
          signature_u: Buffer.from(script_sig.u, "hex"),
          signature_v: Buffer.from(script_sig.v, "hex"),
        },
        sender_offset_public_key: input.output.sender_offset_public_key,
        covenant: Buffer.from([]),
        encrypted_value: input.output.encrypted_value,
        minimum_value_promise: input.output.minimum_value_promise,
      },
      amount: input.amount,
      privateKey: input.privateKey,
      scriptPrivateKey: input.scriptPrivateKey,
    });
  }

  addOutput(amount, features = {}) {
    let key = Math.floor(Math.random() * 500000000000 + 1);
    let privateKey = Buffer.from(toLittleEndian(key, 256)).toString("hex");
    let scriptKey = Math.floor(Math.random() * 500000000000 + 1);
    let scriptPrivateKey = Buffer.from(toLittleEndian(scriptKey, 256)).toString(
      "hex"
    );
    let scriptOffsetPrivateKeyNum = Math.floor(
      Math.random() * 500000000000 + 1
    );
    let scriptOffsetPrivateKey = Buffer.from(
      toLittleEndian(scriptOffsetPrivateKeyNum, 256)
    ).toString("hex");
    let scriptOffsetPublicKeyHex = tari_crypto.pubkey_from_secret(
      scriptOffsetPrivateKey.toString("hex")
    );
    let scriptOffsetPublicKey = Buffer.from(scriptOffsetPublicKeyHex, "hex");

    let nopScriptBytes = Buffer.from([0x73]);
    let covenantBytes = Buffer.from([]);

    let rangeproofFactory = tari_crypto.ExtendedRangeProofFactory.new();
    let rangeproof = rangeproofFactory.create_proof(
      privateKey,
      BigInt(amount)
    ).proof;
    let amount_key = Buffer.from(toLittleEndian(amount, 256)).toString("hex");
    this.kv.new_key("common_nonce_1");
    this.kv.new_key("common_nonce_2");
    let private_nonce_1 = this.kv.private_key("common_nonce_1");
    let private_nonce_2 = this.kv.private_key("common_nonce_2");
    let public_nonce_hex = tari_crypto.commit_private_keys(
      private_nonce_1,
      private_nonce_2
    ).commitment;
    let public_nonce = Buffer.from(public_nonce_hex, "hex");
    let commitment = Buffer.from(
      tari_crypto.commit(privateKey, BigInt(amount)).commitment,
      "hex"
    );
    let encryptedValue = Buffer.concat([
      Buffer.from(toLittleEndian(amount, 64)),
      Buffer.alloc(16),
    ]);
    const outputFeatures = Object.assign({
      output_type: OutputType.STANDARD,
      maturity: 0,
      metadata: [],
      // In case any of these change, update the buildMetaChallenge function
      unique_id: features.unique_id
        ? Buffer.from(features.unique_id, "utf8")
        : null,
      sidechain_feature: null,
      parent_public_key: null,
      asset: null,
      mint_non_fungible: null,
      sidechain_checkpoint: null,
      committee_definition: null,
    });
    let minimumValuePromise = 0;
    let meta_challenge = this.buildMetaChallenge(
      nopScriptBytes,
      outputFeatures,
      scriptOffsetPublicKey,
      public_nonce,
      commitment,
      covenantBytes,
      encryptedValue,
      minimumValuePromise
    );
    let total_key = combineTwoTariKeys(
      scriptOffsetPrivateKey.toString(),
      privateKey.toString()
    );
    let meta_sig = tari_crypto.sign_comsig_challenge_with_nonce(
      amount_key,
      total_key,
      private_nonce_1,
      private_nonce_2,
      meta_challenge.toString("hex")
    );
    let output = {
      amount: amount,
      privateKey: privateKey,
      scriptPrivateKey: scriptPrivateKey,
      scriptOffsetPrivateKey: scriptOffsetPrivateKey,
      output: {
        features: outputFeatures,
        commitment: commitment,
        range_proof: Buffer.from(rangeproof, "hex"),
        script: nopScriptBytes,
        sender_offset_public_key: Buffer.from(scriptOffsetPublicKey, "hex"),
        metadata_signature: {
          public_nonce_commitment: Buffer.from(meta_sig.public_nonce, "hex"),
          signature_u: Buffer.from(meta_sig.u, "hex"),
          signature_v: Buffer.from(meta_sig.v, "hex"),
        },
        covenant: covenantBytes,
        encrypted_value: encryptedValue,
        minimum_value_promise: minimumValuePromise,
      },
    };
    this.outputs.push(output);
    return output;
  }

  getSpendableAmount() {
    let sum = 0;
    this.inputs.forEach((input) => (sum = sum + input.amount));
    return sum - this.fee;
  }

  build() {
    let totalPrivateKey = 0n;
    let script_offset = tari_crypto.secret_key_from_hex_bytes(
      "0000000000000000000000000000000000000000000000000000000000000000"
    );

    this.inputs.forEach((input) => {
      totalPrivateKey -= BigInt(
        littleEndianHexStringToBigEndianHexString(input.privateKey.toString())
      );

      script_offset = tari_crypto.add_secret_keys(
        script_offset,
        input.scriptPrivateKey.toString("hex")
      );
    });
    this.outputs.forEach((output) => {
      totalPrivateKey += BigInt(
        littleEndianHexStringToBigEndianHexString(output.privateKey.toString())
      );
      script_offset = tari_crypto.subtract_secret_keys(
        script_offset,
        output.scriptOffsetPrivateKey
      );
    });

    // We need to check for wrap around as these private keys are supposed to be unsigned integers,
    // but in js these are floats. So we add the little endian number that is required to wrap the number so that it is
    // again positive. This is the (max number -1) that tari_crypto can accommodate.
    if (totalPrivateKey < 0) {
      totalPrivateKey =
        totalPrivateKey +
        BigInt(
          littleEndianHexStringToBigEndianHexString(
            "edd3f55c1a631258d69cf7a2def9de1400000000000000000000000000000010"
          )
        );
    }

    let totalPrivateKeyHex = totalPrivateKey.toString(16);
    while (totalPrivateKeyHex.length < 64) {
      totalPrivateKeyHex = "0" + totalPrivateKeyHex;
    }
    let privateKey =
      littleEndianHexStringToBigEndianHexString(totalPrivateKeyHex);
    while (privateKey.length < 64) {
      privateKey = "0" + privateKey;
    }

    const excess = tari_crypto.commit(privateKey, BigInt(0));
    this.kv.new_key("common_nonce");
    const publicNonce = this.kv.public_key("common_nonce");
    let PublicKeyExcess = tari_crypto.pubkey_from_secret(
      privateKey.toString("hex")
    );
    const challenge = this.buildKernelChallenge(
      publicNonce,
      PublicKeyExcess,
      this.fee,
      this.lockHeight,
      0
    );
    const privateNonce = this.kv.private_key("common_nonce");
    const sig = tari_crypto.sign_challenge_with_nonce(
      privateKey,
      privateNonce,
      challenge
    );
    return {
      offset: Buffer.from(toLittleEndian(0, 256), "hex"),
      script_offset: Buffer.from(script_offset, "hex"),
      body: {
        inputs: this.inputs.map((i) => i.input),
        outputs: this.outputs.map((o) => o.output),
        kernels: [
          {
            features: 0,
            fee: this.fee,
            lock_height: this.lockHeight,
            excess: Buffer.from(excess.commitment, "hex"),
            excess_sig: {
              public_nonce: Buffer.from(sig.public_nonce, "hex"),
              signature: Buffer.from(sig.signature, "hex"),
            },
          },
        ],
      },
    };
  }

  generateCoinbase(value, privateKey, fee, lockHeight) {
    let coinbase = tari_crypto.commit(privateKey, BigInt(value + fee));
    let nopScriptBytes = Buffer.from([0x73]);
    let covenantBytes = Buffer.from([]);
    let scriptOffsetPrivateKeyNum = Math.floor(
      Math.random() * 500000000000 + 1
    );
    let scriptOffsetPrivateKey = Buffer.from(
      toLittleEndian(scriptOffsetPrivateKeyNum, 256)
    ).toString("hex");
    let scriptOffsetPublicKeyHex = tari_crypto.pubkey_from_secret(
      scriptOffsetPrivateKey.toString("hex")
    );
    let scriptOffsetPublicKey = Buffer.from(scriptOffsetPublicKeyHex, "hex");

    let rangeproofFactory = tari_crypto.ExtendedRangeProofFactory.new();
    let rangeproof = rangeproofFactory.create_proof(
      privateKey.toString("hex"),
      BigInt(value + fee)
    ).proof;
    const excess = tari_crypto.commit(privateKey, BigInt(0));
    this.kv.new_key("nonce");
    const public_nonce = this.kv.public_key("nonce");
    let PublicKeyExcess = tari_crypto.pubkey_from_secret(
      privateKey.toString("hex")
    );

    const challenge = this.buildKernelChallenge(
      public_nonce,
      PublicKeyExcess,
      0,
      0,
      1
    );
    const private_nonce = this.kv.private_key("nonce");
    const sig = tari_crypto.sign_challenge_with_nonce(
      privateKey,
      private_nonce,
      challenge
    );

    let amount_key = Buffer.from(toLittleEndian(value + fee, 256)).toString(
      "hex"
    );
    this.kv.new_key("common_nonce_1");
    this.kv.new_key("common_nonce_2");
    let private_nonce_1 = this.kv.private_key("common_nonce_1");
    let private_nonce_2 = this.kv.private_key("common_nonce_2");
    let public_nonce_c_hex = tari_crypto.commit_private_keys(
      private_nonce_1,
      private_nonce_2
    ).commitment;
    let public_nonce_c = Buffer.from(public_nonce_c_hex, "hex");
    let commitment = Buffer.from(
      tari_crypto.commit(privateKey, BigInt(value + fee)).commitment,
      "hex"
    );
    let encryptedValue = Buffer.concat([
      Buffer.from(toLittleEndian(value, 64)),
      Buffer.alloc(16),
    ]);
    let outputFeatures = {
      output_type: OutputType.COINBASE,
      maturity: lockHeight,
      metadata: [],
      // In case any of these change, update the buildMetaChallenge function
      unique_id: null,
      parent_public_key: null,
      asset: null,
      mint_non_fungible: null,
      sidechain_checkpoint: null,
      committee_definition: null,
    };
    let minimumValuePromise = 0;
    let meta_challenge = this.buildMetaChallenge(
      nopScriptBytes,
      outputFeatures,
      scriptOffsetPublicKey,
      public_nonce_c,
      commitment,
      covenantBytes,
      encryptedValue,
      minimumValuePromise
    );
    let total_key = combineTwoTariKeys(
      scriptOffsetPrivateKey.toString(),
      privateKey.toString()
    );
    let meta_sig = tari_crypto.sign_comsig_challenge_with_nonce(
      amount_key,
      total_key,
      private_nonce_1,
      private_nonce_2,
      meta_challenge.toString("hex")
    );

    return {
      outputs: [
        {
          features: outputFeatures,
          commitment: Buffer.from(coinbase.commitment, "hex"),
          range_proof: Buffer.from(rangeproof, "hex"),
          script: nopScriptBytes,
          sender_offset_public_key: Buffer.from(scriptOffsetPublicKey, "hex"),
          metadata_signature: {
            public_nonce_commitment: Buffer.from(meta_sig.public_nonce, "hex"),
            signature_u: Buffer.from(meta_sig.u, "hex"),
            signature_v: Buffer.from(meta_sig.v, "hex"),
          },
          covenant: covenantBytes,
          encrypted_value: encryptedValue,
          minimum_value_promise: minimumValuePromise,
        },
      ],
      kernels: [
        {
          features: 1,
          fee: 0,
          lock_height: 0,
          excess: Buffer.from(excess.commitment, "hex"),
          excess_sig: {
            public_nonce: Buffer.from(sig.public_nonce, "hex"),
            signature: Buffer.from(sig.signature, "hex"),
          },
        },
      ],
    };
  }
}

module.exports = TransactionBuilder;
