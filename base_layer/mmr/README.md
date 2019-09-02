# Merkle Mountain Range

This crate is part of the [Tari Cryptocurrency](https://tari.com) project.

The Merkle mountain range was invented by Peter Todd. More about them can be read
[here](https://github.com/opentimestamps/opentimestamps-server/blob/master/doc/merkle-mountain-range.md) and
[here](https://github.com/mimblewimble/grin/blob/master/doc/mmr.md)

A Merkle mountain range(MMR) is a binary tree where each parent is the concatenated hash of its two children. The leaves
at the bottom of the MMR is the hashes of the data. The MMR allows easy to add and proof of existence inside of the
tree. MMR always tries to have the largest possible single binary tree, so in effect it is possible to have more than
one binary tree. Every time you have to get the merkle root (the single merkle proof of the whole MMR) you have the bag
the peaks of the individual trees, or mountain peaks.