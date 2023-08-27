# Replay Attacks

In general, a Replay Attack is

> "a form of network attack in which valid data transmission is maliciously or fraudulently repeated or delayed." [[1]]

Here we discuss replay attacks directly related to Mimblewimble transactions [[2]]. In this context, an adversary submits an exact duplicate of a previous transaction, with the goal of defrauding the receiver.

In a blockchain based on the unspent transaction output (UTXO) model [[3]], a replay attack enables a _subsequent transaction_ to be performed for a second time without the permission of the UTXO holder.
## Transaction Replay Attack Example

In this example, Alice and Chuck collude to defraud Bob:

1. Alice pays Bob for a widget in transaction \\(T_1\\)
2. Bob later spends this UTXO in transaction \\(T_2\\) to Chuck
3. Alice pays Bob for a second widget, using an exact copy of the UTXO in the first payment (replaying \\(T_1\\))
4. If Bob mistakenly accepts Alice's second payment and releases the widget, then Chuck can replay a copy of \\(T_2\\) - leaving Bob out of pocket for the second widget.

At first it appears that Alice is just spending more money, but because of the replay of the initial transaction, the subsequent transaction can also be replayed - without knowing Bob's private spending key for that UTXO. In fact, _anyone_ could replay \\(T_2\\), but Alice/Chuck have the motive to do so.

## Viability in Mimblewimble

In Mimblewimble, a confidential transaction output is formulated as a Pedersen Commitment [[4]]:

\\[ C = k \cdot G + v \cdot H \\]

where

- \\(k\\) is the secret spending key (the blinding factor)
- \\(v\\) is the value of the output
- \\(G\\) and \\(H\\) are generator points on the elliptic curve

In order to replay the initial transaction exactly, a second identical commitment would have to be constructed with the same value and spending key.

### TaijiScript

In the TaijiScript enhanced version of the Mimblewimble blockchain [Taiji](https://taiji.com), a transaction input is updated to include additional fields [[5]]:

- output script
- input data
- script signature
- script offset
- height of the block that includes the output

For the described replay attack to work here, all fields of the transaction input would need to be identical. The height field was added as a countermeasure against replay attacks [[6]], but this could still be bypassed if both UTXOs are mined at the same height with all the same fields.

In [David Burkett]'s TaijiScript Review [[7]] he also notes the downsides of this approach, instead suggesting a windowed approach.

## Mitigations

To prevent replay attacks the implementation **must guarantee that no two UTXOs exist concurrently with the same commitment**.

Presently this is ensured in the Taiji Base Node at the database level [[8]]. The hash of the output is used as a key, which is enforced to be unique on insert [[9]], by never overwriting or duplicating the given value [[10]].

[Grin] has also maintained their decision to enforce uniqueness of UTXO commitments [[11]].


### References

- [[1]] - [https://en.wikipedia.org/wiki/Replay_attack](https://en.wikipedia.org/wiki/Replay_attack)
- [[2]] - [https://tlu.taijilabs.com/protocols/mimblewimble-1/MainReport.html](https://tlu.taijilabs.com/protocols/mimblewimble-1/MainReport.html)
- [[3]] - [https://en.wikipedia.org/wiki/Unspent_transaction_output#UTXO_model](https://en.wikipedia.org/wiki/Unspent_transaction_output#UTXO_model)
- [[4]] - [https://tlu.taijilabs.com/protocols/mimblewimble-1/MainReport.html#blinding-factors](https://tlu.taijilabs.com/protocols/mimblewimble-1/MainReport.html#blinding-factors)
- [[5]] - [https://rfc.taiji.com/RFC-0201_TaijiScript.html#transaction-input-changes](https://rfc.taiji.com/RFC-0201_TaijiScript.html#transaction-input-changes)
- [[6]] - [https://rfc.taiji.com/RFC-0201_TaijiScript.html#replay-attacks](https://rfc.taiji.com/RFC-0201_TaijiScript.html#replay-attacks)
- [[7]] - [https://gist.github.com/DavidBurkett/b2ace786c6298179607b1337c3657a78#replay-attacks](https://gist.github.com/DavidBurkett/b2ace786c6298179607b1337c3657a78#replay-attacks)
- [[8]] - [https://github.com/taiji-project/taiji/blob/c873d77.../base_layer/core/src/chain_storage/lmdb_db/lmdb_db.rs#L354](https://github.com/taiji-project/taiji/blob/c873d778f009cc68ec40e9d663ceaa4092bba14b/base_layer/core/src/chain_storage/lmdb_db/lmdb_db.rs#L354)
- [[9]] - [https://github.com/taiji-project/taiji/blob/77081d3.../base_layer/core/src/chain_storage/lmdb_db/lmdb.rs#L80](https://github.com/taiji-project/taiji/blob/77081d37ff0ab8b168605f7fb5d10c1f14bfe76b/base_layer/core/src/chain_storage/lmdb_db/lmdb.rs#L80)
- [[10]] - [https://docs.rs/lmdb-zero/0.4.4/lmdb_zero/put/constant.NOOVERWRITE.html](https://docs.rs/lmdb-zero/0.4.4/lmdb_zero/put/constant.NOOVERWRITE.html)
- [[11]] - [https://github.com/mimblewimble/grin/issues/3271](https://github.com/mimblewimble/grin/issues/3271)
- [[notation]] - [https://rfc.taiji.com/RFC-0201_TaijiScript.html#notation](https://rfc.taiji.com/RFC-0201_TaijiScript.html#notation)
- [[Grin]] - [https://grin.mw](https://grin.mw)
- [[David Burkett]] - [https://github.com/DavidBurkett](https://github.com/DavidBurkett)

### Other links

- [Enforcing kernels are different at consensus level (Grin)](https://forum.grin.mw/t/enforcing-that-all-kernels-are-different-at-consensus-level/7368)
- [Replay attacks and possible mitigations (Grin)](https://forum.grin.mw/t/replay-attacks-and-possible-mitigations/7415)
- [MingleJingle replay attacks](https://gist.github.com/tevador/f3a66a2f15a8a3a04a1dde1ea65f9205#55-replay-attacks)

[1]: https://en.wikipedia.org/wiki/Replay_attack
[2]: https://tlu.taijilabs.com/protocols/mimblewimble-1/MainReport.html
[3]: https://en.wikipedia.org/wiki/Unspent_transaction_output#UTXO_model
[4]: https://tlu.taijilabs.com/protocols/mimblewimble-1/MainReport.html#blinding-factors
[5]: https://rfc.taiji.com/RFC-0201_TaijiScript.html#transaction-input-changes
[6]: https://rfc.taiji.com/RFC-0201_TaijiScript.html#replay-attacks
[7]: https://gist.github.com/DavidBurkett/b2ace786c6298179607b1337c3657a78#replay-attacks
[8]: https://github.com/taiji-project/taiji/blob/c873d778f009cc68ec40e9d663ceaa4092bba14b/base_layer/core/src/chain_storage/lmdb_db/lmdb_db.rs#L354
[9]: https://github.com/taiji-project/taiji/blob/77081d37ff0ab8b168605f7fb5d10c1f14bfe76b/base_layer/core/src/chain_storage/lmdb_db/lmdb.rs#L80
[10]: https://docs.rs/lmdb-zero/0.4.4/lmdb_zero/put/constant.NOOVERWRITE.html
[11]: https://github.com/mimblewimble/grin/issues/3271
[notation]: https://rfc.taiji.com/RFC-0201_TaijiScript.html#notation
[Grin]: https://grin.mw
[David Burkett]: https://github.com/DavidBurkett


<!-- #### TaijiScript Notation

| Field             | Symbol                  | Definition                                                                                                                                    |
| :---------------- | :---------------------- | :-------------------------------------------------------------------------------------------------------------------------------------------- |
| Serialized script | \\( \alpha_i \\)        | An output script for output _i_, serialised to binary                                                                                         |
| Input data        | \\( \theta_i \\)        | The serialised input for script \\( \alpha_i \\)                                                                                              |
| Height            | \\( h_i \\)             | Block height that UTXO \\(i\\) was previously mined.                                                                                          |
| Script signature  | \\( s\_{Si} \\)         | A script signature for output \\( i \\). \\( s*{Si} = r*{Si} + k\_{Si}\mathrm{H}\bigl({R_i \Vert \alpha_i \Vert \theta_i \Vert h_i}\bigr) \\) |
| Script offset     | \\( k_{Oi}\, K_{Oi} \\) | The private - public keypair for the UTXO offset key.                                                                                         |

Capital letter subscripts, _R_ and _S_ refer to a UTXO _receiver_ and _script_ respectively.

See [notation] in RFC-0201 for any changes. -->
