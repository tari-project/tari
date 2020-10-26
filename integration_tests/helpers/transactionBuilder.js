
var crypto = require('crypto');
var hex64 = require('hex64');
var tari_crypto = require('tari_crypto');

class TransactionBuilder {
    constructor() {

    }

    generatePrivateKey(id) {
        let kv = tari_crypto.KeyRing.new();
        kv.new_key(id);
        return kv.private_key(id);
    }

    generateCoinbase(value, privateKey, maturity) {
        let coinbase = tari_crypto.commit(privateKey, value);
        let rangeproofFactory = tari_crypto.RangeProofFactory.new();
        let rangeproof = rangeproofFactory.create_proof(privateKey, value).proof;
        let excess = tari_crypto.commit(privateKey, BigInt(0));
        let challenge = buildChallenge()
        let sig = tari_crypto.sign(privateKey, "hello world");
        let outputFeatures ={
            flags: 1,
            maturity: maturity
        };
        return {
            outputs: [{
                features: outputFeatures,
                commitment: Buffer.from(coinbase.commitment,'hex'),
               range_proof: Buffer.from(rangeproof, 'hex')
            }],
            kernels: [
                {
                    features: 1,
                    fee: 0,
                    lock_height: maturity,
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

module.exports = TransactionBuilder;
