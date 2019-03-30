# Tari Crypto

This crate is part of the [Tari Cryptocurrency](https://tari.com) project.

Major features of this library include:

* Pedersen commitments
* Schnorr Signatures
* Generic Public and Secret Keys
* [Musig!](https://blockstream.com/2018/01/23/musig-key-aggregation-schnorr-signatures/)

The `tari_crypto` crate makes heavy use of the excellent [Dalek](https://github.com/dalek-cryptography/curve25519-dalek)
libraries. The default implementation for Tari ECC is the [Ristretto255 curve](https://ristretto.group).