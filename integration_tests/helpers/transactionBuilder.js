var tari_crypto = require("tari_crypto");
var { blake2bInit, blake2bUpdate, blake2bFinal } = require("blakejs");
const { toLittleEndian, calculateBeta } = require("../helpers/util");

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

  buildScriptChallenge(publicNonce, script, input_data) {
    var KEY = null; // optional key
    var OUTPUT_LENGTH = 32; // bytes
    var context = blake2bInit(OUTPUT_LENGTH, KEY);
    let buff = Buffer.from(publicNonce, "hex");
    blake2bUpdate(context, buff);
    blake2bUpdate(context, script);
    blake2bUpdate(context, input_data);
    // blake2bUpdate(context, height);
    let final = blake2bFinal(context);
    return Buffer.from(final).toString("hex");
  }

  hashOutput(features, commitment, script, script_offset_public_key) {
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
    blake2bUpdate(context, script_offset_public_key);
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
    this.kv.new_key("common_nonce");
    let public_nonce = this.kv.public_key("common_nonce");
    let challenge = this.buildScriptChallenge(
      public_nonce,
      nopScriptBytes,
      input_data,
      0
    );
    let private_nonce = this.kv.private_key("common_nonce");
    let script_sig = tari_crypto.sign_challenge_with_nonce(
      input.scriptPrivateKey,
      private_nonce,
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
          public_nonce: Buffer.from(script_sig.public_nonce, "hex"),
          signature: Buffer.from(script_sig.signature, "hex"),
        },
        script_offset_public_key: input.output.script_offset_public_key,
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
    let key = Math.floor(Math.random() * 500 + 1);
    let privateKey = Buffer.from(toLittleEndian(key, 256)).toString("hex");
    let scriptKey = Math.floor(Math.random() * 500 + 1);
    let scriptPrivateKey = Buffer.from(toLittleEndian(scriptKey, 256)).toString(
      "hex"
    );
    let scriptOffsetPrivateKeyNum = Math.floor(Math.random() * 500 + 1);
    let scriptOffsetPrivateKey = Buffer.from(
      toLittleEndian(scriptOffsetPrivateKeyNum, 256)
    ).toString("hex");
    let scriptOffsetPublicKey = tari_crypto.pubkey_from_secret(
      scriptOffsetPrivateKey.toString("hex")
    );
    let nopScriptBytes = Buffer.from([0x73]);

    let beta = calculateBeta(
      nopScriptBytes,
      outputFeatures,
      scriptOffsetPublicKey
    );

    let beta_key = tari_crypto.secret_key_from_hex_bytes(beta.toString("hex"));
    let new_range_proof_key = tari_crypto.add_secret_keys(beta_key, privateKey);

    let rangeproofFactory = tari_crypto.RangeProofFactory.new();
    let rangeproof = rangeproofFactory.create_proof(
      new_range_proof_key,
      BigInt(amount)
    ).proof;

    let output = {
      amount: amount,
      privateKey: privateKey,
      scriptPrivateKey: scriptPrivateKey,
      scriptOffsetPrivateKey: scriptOffsetPrivateKey,
      output: {
        features: outputFeatures,
        commitment: Buffer.from(
          tari_crypto.commit(privateKey, BigInt(amount)).commitment,
          "hex"
        ),
        range_proof: Buffer.from(rangeproof, "hex"),
        script: nopScriptBytes,
        script_offset_public_key: Buffer.from(scriptOffsetPublicKey, "hex"),
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
      totalPrivateKey -= BigInt("0x" + input.privateKey.toString());

      script_offset = tari_crypto.add_secret_keys(
        script_offset,
        input.scriptPrivateKey.toString("hex")
      );
    });
    this.outputs.forEach((output) => {
      totalPrivateKey += BigInt("0x" + output.privateKey.toString());
      let output_hash = this.hashOutput(
        output.output.features,
        output.output.commitment,
        output.output.script,
        output.output.script_offset_public_key
      );
      let kU = tari_crypto.secret_key_from_hex_bytes(
        output_hash.toString("hex")
      );
      kU = tari_crypto.multiply_secret_keys(output.scriptOffsetPrivateKey, kU);
      script_offset = tari_crypto.subtract_secret_keys(script_offset, kU);
    });
    // Assume low numbers....

    let privateKey = totalPrivateKey.toString(16);
    // we need to pad 0's in front
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
    let scriptOffsetPublicKey = Buffer.from(
      "0000000000000000000000000000000000000000000000000000000000000000",
      "hex"
    );
    let beta = calculateBeta(
      nopScriptBytes,
      outputFeatures,
      scriptOffsetPublicKey
    );

    let beta_key = tari_crypto.secret_key_from_hex_bytes(beta.toString("hex"));
    let new_range_proof_key = tari_crypto.add_secret_keys(beta_key, privateKey);

    let rangeproofFactory = tari_crypto.RangeProofFactory.new();
    let rangeproof = rangeproofFactory.create_proof(
      new_range_proof_key.toString("hex"),
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

    return {
      outputs: [
        {
          features: outputFeatures,
          commitment: Buffer.from(coinbase.commitment, "hex"),
          range_proof: Buffer.from(rangeproof, "hex"),
          script: nopScriptBytes,
          script_offset_public_key: scriptOffsetPublicKey,
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
