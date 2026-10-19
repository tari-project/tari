# Fixtures

## How to update the commit proof fixture

`commit_proof.json` is a JSON-serialized `SidechainBlockCommitProof` produced by the ootle repo.

Its certificates are signed by all four members of the committee in `tests/support/mod.rs::fixture_committee`, so
each carries the signer bitmap `0f` and its signatures in ascending order of the signers' public keys.
