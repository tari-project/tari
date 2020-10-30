var crypto = require('crypto');
var hex64 = require('hex64');
var tari_crypto = require('tari_crypto');
var {blake2bInit, blake2bUpdate, blake2bFinal} = require('blakejs');
const {toLittleEndian} = require("../helpers/util");

class TransactionBuilder {
    constructor() {
        this.kv = tari_crypto.KeyRing.new();
    }

    generatePrivateKey(id) {
        this.kv.new_key(id);
        return this.kv.private_key(id);
    }

    buildChallenge(publicNonce, fee, lockHeight) {
        var KEY = null // optional key
        var OUTPUT_LENGTH = 32 // bytes
        var context = blake2bInit(OUTPUT_LENGTH, KEY);
        let buff = Buffer.from(publicNonce, "hex");
        blake2bUpdate(context,buff);
        blake2bUpdate(context,toLittleEndian(fee, 64));
        blake2bUpdate(context,toLittleEndian(lockHeight, 64));
        let final = blake2bFinal(context);
        return Buffer.from(final).toString('hex');
    }

    generateCoinbase(value, privateKey, fee, lockHeight) {
        let coinbase = tari_crypto.commit(privateKey, value);
        let rangeproofFactory = tari_crypto.RangeProofFactory.new();
        let rangeproof = rangeproofFactory.create_proof(privateKey, value).proof;
        let excess = tari_crypto.commit(privateKey, BigInt(0));
        this.kv.new_key("nonce");
        let public_nonce = this.kv.public_key("nonce");
        let challenge = this.buildChallenge(public_nonce, fee, lockHeight);
        let private_nonce = this.kv.private_key("nonce");
        let sig = tari_crypto.sign_challenge_with_nonce(privateKey, private_nonce, challenge);
        let outputFeatures ={
            flags: 1,
            maturity: lockHeight
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

module.exports = TransactionBuilder;
