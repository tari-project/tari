const tari_crypto = require('tari_crypto')
const { blake2bInit, blake2bUpdate, blake2bFinal } = require('blakejs')
const { toLittleEndian, hexSwitchEndianness } = require('../helpers/util')

class TransactionBuilder {
  constructor () {
    this.kv = tari_crypto.KeyRing.new()
    this.inputs = []
    this.outputs = []
    this.fee = 100
    this.lockHeight = 0
  }

  generatePrivateKey (id) {
    this.kv.new_key(id)
    return this.kv.private_key(id)
  }

  buildChallenge (publicNonce, fee, lockHeight) {
    const KEY = null // optional key
    const OUTPUT_LENGTH = 32 // bytes
    const context = blake2bInit(OUTPUT_LENGTH, KEY)
    const buff = Buffer.from(publicNonce, 'hex')
    blake2bUpdate(context, buff)
    blake2bUpdate(context, toLittleEndian(fee, 64))
    blake2bUpdate(context, toLittleEndian(lockHeight, 64))
    const final = blake2bFinal(context)
    return Buffer.from(final).toString('hex')
  }

  changeFee (fee) {
    this.fee = fee
  }

  addInput (input) {
    this.inputs.push({
      input: input.output,
      amount: input.amount,
      privateKey: input.privateKey
    })
  }

  addOutput (amount) {
    const outputFeatures = {
      flags: 0,
      maturity: 0
    }
    const key = Math.floor(Math.random() * 500 + 1)
    const privateKey = Buffer.from(toLittleEndian(key, 256)).toString('hex')
    const rangeproofFactory = tari_crypto.RangeProofFactory.new()
    const rangeproof = rangeproofFactory.create_proof(privateKey, BigInt(amount))
      .proof
    const output = {
      amount: amount,
      privateKey: privateKey,
      output: {
        features: outputFeatures,
        commitment: Buffer.from(
          tari_crypto.commit(privateKey, BigInt(amount)).commitment,
          'hex'
        ),
        range_proof: Buffer.from(rangeproof, 'hex')
      }
    }
    this.outputs.push(output)
    return output
  }

  getSpendableAmount () {
    let sum = 0
    this.inputs.forEach((input) => (sum = sum + input.amount))
    return sum - this.fee
  }

  build () {
    let totalPrivateKey = 0n

    this.outputs.forEach(
      (output) =>
        (totalPrivateKey += BigInt('0x' + output.privateKey.toString()))
    )
    this.inputs.forEach(
      (input) => (totalPrivateKey -= BigInt('0x' + input.privateKey.toString()))
    )
    // Assume low numbers....

    let PrivateKey = totalPrivateKey.toString(16)
    // we need to pad 0's in front
    while (PrivateKey.length < 64) {
      PrivateKey = '0' + PrivateKey
    }
    const excess = tari_crypto.commit(PrivateKey, BigInt(0))
    const nonce = this.kv.new_key('common_nonce')
    const public_nonce = this.kv.public_key('common_nonce')
    const challenge = this.buildChallenge(
      public_nonce,
      this.fee,
      this.lockHeight
    )
    const private_nonce = this.kv.private_key('common_nonce')
    const sig = tari_crypto.sign_challenge_with_nonce(
      PrivateKey,
      private_nonce,
      challenge
    )

    return {
      offset: Buffer.from(toLittleEndian(0, 256), 'hex'),
      body: {
        inputs: this.inputs.map((i) => i.input),
        outputs: this.outputs.map((o) => o.output),
        kernels: [
          {
            features: 0,
            fee: this.fee,
            lock_height: this.lockHeight,
            excess: Buffer.from(excess.commitment, 'hex'),
            excess_sig: {
              public_nonce: Buffer.from(sig.public_nonce, 'hex'),
              signature: Buffer.from(sig.signature, 'hex')
            }
          }
        ]
      }
    }
  }

  generateCoinbase (value, privateKey, fee, lockHeight) {
    const coinbase = tari_crypto.commit(privateKey, BigInt(value + fee))
    const rangeproofFactory = tari_crypto.RangeProofFactory.new()
    const rangeproof = rangeproofFactory.create_proof(
      privateKey,
      BigInt(value + fee)
    ).proof
    const excess = tari_crypto.commit(privateKey, BigInt(0))
    this.kv.new_key('nonce')
    const public_nonce = this.kv.public_key('nonce')
    const challenge = this.buildChallenge(public_nonce, 0, lockHeight)
    const private_nonce = this.kv.private_key('nonce')
    const sig = tari_crypto.sign_challenge_with_nonce(
      privateKey,
      private_nonce,
      challenge
    )
    const outputFeatures = {
      flags: 1,
      maturity: lockHeight
    }
    return {
      outputs: [
        {
          features: outputFeatures,
          commitment: Buffer.from(coinbase.commitment, 'hex'),
          range_proof: Buffer.from(rangeproof, 'hex')
        }
      ],
      kernels: [
        {
          features: 1,
          fee: 0,
          lock_height: lockHeight,
          excess: Buffer.from(excess.commitment, 'hex'),
          excess_sig: {
            public_nonce: Buffer.from(sig.public_nonce, 'hex'),
            signature: Buffer.from(sig.signature, 'hex')
          }
        }
      ]
    }
  }
}

module.exports = TransactionBuilder
