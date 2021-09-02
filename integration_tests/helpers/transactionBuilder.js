var tari_crypto = require("tari_crypto");
var { blake2bInit, blake2bUpdate, blake2bFinal } = require("blakejs");
const {
  toLittleEndian,
  littleEndianHexStringToBigEndianHexString,
  combineTwoTariKeys,
} = require("../helpers/util");

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

  buildChallenge(publicNonce, fee, lockHeight) {
    const KEY = null; // optional key
    const OUTPUT_LENGTH = 32; // bytes
    const context = blake2bInit(OUTPUT_LENGTH, KEY);
    const buff = Buffer.from(publicNonce, "hex");
    blake2bUpdate(context, buff);
    blake2bUpdate(context, toLittleEndian(fee, 64));
    blake2bUpdate(context, toLittleEndian(lockHeight, 64));
    const final = blake2bFinal(context);
    return Buffer.from(final).toString("hex");
  }

  buildMetaChallenge(
    script,
    features,
    scriptOffsetPublicKey,
    publicNonce,
    commitment
  ) {
    const KEY = null; // optional key
    const OUTPUT_LENGTH = 32; // bytes
    const context = blake2bInit(OUTPUT_LENGTH, KEY);
    const buff_nonce = Buffer.from(publicNonce, "hex");
    const buff_key = Buffer.from(scriptOffsetPublicKey, "hex");
    let flags = Buffer.alloc(1);
    flags[0] = features.flags;
    let features_buffer = Buffer.concat([
      flags,
      toLittleEndian(parseInt(features.maturity), 64),
    ]);
    blake2bUpdate(context, buff_nonce);
    blake2bUpdate(context, script);
    blake2bUpdate(context, features_buffer);
    blake2bUpdate(context, buff_key);
    blake2bUpdate(context, commitment);
    const final = blake2bFinal(context);
    return Buffer.from(final).toString("hex");
  }

  buildScriptChallenge(
    publicNonce,
    script,
    input_data,
    public_key,
    commitment
  ) {
    var KEY = null; // optional key
    var OUTPUT_LENGTH = 32; // bytes
    var context = blake2bInit(OUTPUT_LENGTH, KEY);
    let buff_publicNonce = Buffer.from(publicNonce, "hex");
    let buff_public_key = Buffer.from(public_key, "hex");
    blake2bUpdate(context, buff_publicNonce);
    blake2bUpdate(context, script);
    blake2bUpdate(context, input_data);
    blake2bUpdate(context, buff_public_key);
    blake2bUpdate(context, commitment);
    let final = blake2bFinal(context);
    return Buffer.from(final).toString("hex");
  }

  hashOutput(features, commitment, script, sender_offset_public_key) {
    var KEY = null; // optional key
    var OUTPUT_LENGTH = 32; // bytes
    var context = blake2bInit(OUTPUT_LENGTH, KEY);
    let flags = Buffer.alloc(1);
    flags[0] = features.flags;
    let features_buffer = Buffer.concat([
      flags,
      toLittleEndian(parseInt(features.maturity), 64),
    ]);
    blake2bUpdate(context, features_buffer);
    blake2bUpdate(context, commitment);
    blake2bUpdate(context, script);
    blake2bUpdate(context, sender_offset_public_key);
    let final = blake2bFinal(context);
    return Buffer.from(final).toString("hex");
  }

  changeFee(fee) {
    this.fee = fee;
  }

  addInput(input) {
    let nopScriptBytes = Buffer.from([0x73]);
    let scriptPublicKey = tari_crypto.pubkey_from_secret(
      input.scriptPrivateKey.toString("hex")
    );
    // The 0x04 is type code for a pubkey in TariScript
    let input_data = Buffer.concat([
      Buffer.from([0x04]),
      Buffer.from(scriptPublicKey, "hex"),
    ]);
    this.kv.new_key("common_nonce_1");
    this.kv.new_key("common_nonce_2");
    let private_nonce_1 = this.kv.private_key("common_nonce_1");
    let private_nonce_2 = this.kv.private_key("common_nonce_2");
    let public_nonce = tari_crypto.commit_private_keys(
      private_nonce_1,
      private_nonce_2
    ).commitment;
    let challenge = this.buildScriptChallenge(
      public_nonce,
      nopScriptBytes,
      input_data,
      scriptPublicKey,
      input.output.commitment
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
      challenge
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
      },
      amount: input.amount,
      privateKey: input.privateKey,
      scriptPrivateKey: input.scriptPrivateKey,
    });
  }

  addOutput(amount) {
    const outputFeatures = {
      flags: 0,
      maturity: 0,
    };
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
    let scriptOffsetPublicKey = tari_crypto.pubkey_from_secret(
      scriptOffsetPrivateKey.toString("hex")
    );

    let nopScriptBytes = Buffer.from([0x73]);

    let rangeproofFactory = tari_crypto.RangeProofFactory.new();
    let rangeproof = rangeproofFactory.create_proof(
      privateKey,
      BigInt(amount)
    ).proof;
    let amount_key = Buffer.from(toLittleEndian(amount, 256)).toString("hex");
    this.kv.new_key("common_nonce_1");
    this.kv.new_key("common_nonce_2");
    let private_nonce_1 = this.kv.private_key("common_nonce_1");
    let private_nonce_2 = this.kv.private_key("common_nonce_2");
    let public_nonce = tari_crypto.commit_private_keys(
      private_nonce_1,
      private_nonce_2
    ).commitment;
    let commitment = Buffer.from(
      tari_crypto.commit(privateKey, BigInt(amount)).commitment,
      "hex"
    );
    let meta_challenge = this.buildMetaChallenge(
      nopScriptBytes,
      outputFeatures,
      scriptOffsetPublicKey,
      public_nonce,
      commitment
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
      meta_challenge
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
    const challenge = this.buildChallenge(
      publicNonce,
      this.fee,
      this.lockHeight
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
    let outputFeatures = {
      flags: 1,
      maturity: lockHeight,
    };
    let scriptOffsetPrivateKeyNum = Math.floor(
      Math.random() * 500000000000 + 1
    );
    let scriptOffsetPrivateKey = Buffer.from(
      toLittleEndian(scriptOffsetPrivateKeyNum, 256)
    ).toString("hex");
    let scriptOffsetPublicKey = tari_crypto.pubkey_from_secret(
      scriptOffsetPrivateKey.toString("hex")
    );

    let rangeproofFactory = tari_crypto.RangeProofFactory.new();
    let rangeproof = rangeproofFactory.create_proof(
      privateKey.toString("hex"),
      BigInt(value + fee)
    ).proof;
    const excess = tari_crypto.commit(privateKey, BigInt(0));
    this.kv.new_key("nonce");
    const public_nonce = this.kv.public_key("nonce");
    const challenge = this.buildChallenge(public_nonce, 0, lockHeight);
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
    let public_nonce_c = tari_crypto.commit_private_keys(
      private_nonce_1,
      private_nonce_2
    ).commitment;
    let commitment = Buffer.from(
      tari_crypto.commit(privateKey, BigInt(value + fee)).commitment,
      "hex"
    );
    let meta_challenge = this.buildMetaChallenge(
      nopScriptBytes,
      outputFeatures,
      scriptOffsetPublicKey,
      public_nonce_c,
      commitment
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
      meta_challenge
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
        },
      ],
      kernels: [
        {
          features: 1,
          fee: 0,
          lock_height: lockHeight,
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
